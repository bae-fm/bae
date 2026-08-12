//! The import sidebar's rows, decided once in core.
//!
//! The sidebar asks the same four questions of every candidate — which tab it
//! belongs to, which Needs-you group it joins, what it leads with, and whether
//! it takes a bulk-import checkbox — and every one of them is a rule rather
//! than a rendering. [`crate::identify::view`] is the precedent: shape the
//! state for the surfaces once, here, so both desktop UIs render the same
//! decisions instead of each re-deriving them from a [`FolderCandidate`] and a
//! [`TerminalVerdict`].
//!
//! **Nothing here formats text.** Years, counts, durations and byte sizes cross
//! as numbers, and a disagreement crosses as its own [`NeedsYou`] variant
//! carrying its operands, so each platform builds the sentence in its own
//! locale.
//!
//! **Nothing here re-classifies.** [`crate::identify::ready::classify`] already
//! answers what the queue needs from the user; this module decides where that
//! answer puts the row and what the row shows.

use super::folder_scanner::{
    FolderCandidate, FolderReleaseBoundary, FolderReleaseDecisionKey, InvalidCandidate,
    ResolvedFolderReleaseBoundary,
};
use super::handle::ImportServiceHandle;
use super::search::{ImportSearchReleaseDetail, MetadataResult, SourceTracks};
use super::types::{IdentityChoice, MetadataSource};
use super::{
    CandidateImportStatusSnapshot, FolderImportCandidateSnapshot, ImportCandidatesSnapshot,
    WatchedFolderScanStatus,
};
use crate::db::{DbImportCandidateState, LibraryCheck, LibraryStatus};
use crate::identify::verdict::decode_stored;
use crate::identify::{
    classify, IdentifyState, NeedsYou, QueueClassification, ResultProvenance, TerminalVerdict,
};
use crate::library::{LibraryError, LibraryManager};
use std::collections::{HashMap, HashSet};

mod model;

pub use model::*;

/// Which tab a candidate belongs to, and — under Needs you — why.
///
/// A total function of four facts core already holds, checked in one order:
///
/// 1. **An import in flight outranks everything**, including the library
///    check: the release row lands partway through an import, so `is_added`
///    flips before the import is finished, and a row that reads Done then says
///    the folder is in the library while its files are still being copied.
/// 2. **Then Done**, which is an import that finished — completed or failed —
///    or a folder a previous session already imported. Not awaiting triage,
///    whatever its verdict says and whether or not it was ever skipped.
/// 3. **Then Skipped**, which is a decision the user already made.
/// 4. **Then what is known about it**, which is the only thing that can put a
///    row in Ready.
///
/// **A candidate with no verdict yet is Needs you, not Ready** — the design
/// mockup stacks the "still identifying" group under Ready, and that is the
/// side that is wrong. Ready's count is what a bulk import would act on, so
/// admitting rows nothing is known about turns the one number on the pane that
/// has to be exact into an overstatement, and makes it move on its own while
/// someone reads it. Those rows would also be the only Ready rows with no
/// checkbox, contradicting the design's own rule that Ready is where
/// multi-select lives. And it is worst at the moment it matters most: on a
/// first launch *every* candidate is unanswered, so Ready would open full of
/// dimmed, uncheckable rows with Needs you empty — the exact inverse of the
/// truth. Under Needs you, Ready starts empty and fills as verdicts land, which
/// is the signal a person actually wants, and a still-identifying row leaves
/// its group by itself without anyone answering it.
pub fn place(
    skipped: bool,
    is_added: bool,
    import_status: Option<&CandidateImportStatusSnapshot>,
    answer: &CandidateAnswer,
) -> TriagePlacement {
    // Spelled out rather than `is_some()`: each variant answers "has this
    // import finished" differently, and a new one should have to be placed
    // here on purpose rather than inherited by an `_`.
    let import_finished = match import_status {
        Some(CandidateImportStatusSnapshot::Importing { .. }) => return TriagePlacement::Importing,
        Some(
            CandidateImportStatusSnapshot::Complete { .. }
            | CandidateImportStatusSnapshot::Error { .. },
        ) => true,
        None => false,
    };
    if is_added || import_finished {
        return TriagePlacement::Done;
    }
    if skipped {
        return TriagePlacement::Skipped;
    }
    let reason = match answer {
        CandidateAnswer::Classified(QueueClassification::Ready) => return TriagePlacement::Ready,
        CandidateAnswer::Classified(QueueClassification::NeedsYou(needs_you)) => {
            NeedsYouReason::Disagreement(needs_you.clone())
        }
        CandidateAnswer::Unanswered(phase) => NeedsYouReason::StillIdentifying { phase: *phase },
    };
    TriagePlacement::NeedsYou {
        group: NeedsYouGroup::of(&reason),
        reason,
    }
}

/// Shape one candidate's row.
fn row(
    snapshot: &FolderImportCandidateSnapshot,
    answer: Option<&Answered>,
    picked: Option<&Picked>,
) -> TriageRow {
    let FolderCandidate {
        path,
        name,
        watched_folder_path,
        files: _,
        display_path,
        resolved_boundaries,
        combine_ancestor_key,
        scope: _,
        ..
    } = &snapshot.candidate;
    let import_status = snapshot.runtime.import_status.clone();
    let actionable_answer = answer.filter(|_| snapshot.actionable);
    let known = match actionable_answer {
        Some(answer) => CandidateAnswer::Classified(answer.classification.clone()),
        None => CandidateAnswer::Unanswered(IdentifyPhase::of(&snapshot.runtime.identify_state)),
    };
    let placement = place(
        snapshot.skipped,
        snapshot.is_added,
        import_status.as_ref(),
        &known,
    );
    let picked = picked.filter(|_| snapshot.actionable);
    TriageRow {
        candidate_key: path.to_string_lossy().into_owned(),
        folder_name: name.clone(),
        watched_folder_path: watched_folder_path.clone(),
        display_path: display_path.clone(),
        resolved_boundaries: resolved_boundaries.clone(),
        combine_ancestor_key: combine_ancestor_key.clone(),
        actionable: snapshot.actionable,
        selectable: snapshot.actionable && matches!(placement, TriagePlacement::Ready),
        // The identity the candidate is settled on, which is the user's pick
        // wherever they made one and identification's own answer otherwise.
        matched: match picked {
            Some(picked) => picked.release.clone(),
            None => actionable_answer.and_then(|answer| MatchedRelease::of(&answer.verdict)),
        },
        placement,
        import_status,
        claim: picked.map(|picked| picked.pick.choice()),
        picked: picked.map(|picked| picked.pick.clone()),
    }
}

/// Every candidate's row and the four tab counts, in one pass.
///
/// `answers` is keyed by content hash, which is what the stored rows are keyed
/// by; a candidate with no entry has no verdict yet.
pub fn project(
    snapshot: ImportCandidatesSnapshot,
    answers: &HashMap<(String, u64), Answered>,
    picks: &HashMap<(String, u64), Picked>,
) -> TriageQueue {
    let ImportCandidatesSnapshot {
        folder_candidates,
        runtime_candidates: _,
        invalid_candidates,
        watched_folders,
        boundaries,
        folder_scan_statuses,
    } = snapshot;

    let actionable_candidates: Vec<_> = folder_candidates
        .iter()
        .filter(|candidate| candidate.actionable)
        .collect();
    let mut rows = Vec::with_capacity(actionable_candidates.len());
    let mut counts = TriageTabCounts {
        skipped: invalid_candidates.len() as u32,
        ..TriageTabCounts::default()
    };
    for candidate in actionable_candidates {
        let candidate_identity = (
            candidate.candidate.files.content_hash(),
            candidate.candidate.file_edit_revision,
        );
        let row = row(
            candidate,
            answers.get(&candidate_identity),
            picks.get(&candidate_identity),
        );
        counts.bump(row.placement.tab());
        rows.push(row);
    }
    counts.needs_you += boundaries.len() as u32;
    let sections = project_sections(&watched_folders, &rows, &invalid_candidates, &boundaries);
    TriageQueue {
        sections,
        counts,
        folder_scan_statuses,
    }
}

fn group_for(
    watched_folder_path: &str,
    display_path: &str,
    grouped_roots: &HashSet<(String, String)>,
) -> Option<TriageGroup> {
    let mut components = display_path
        .split('/')
        .filter(|component| !component.is_empty());
    let first = components.next()?;
    if components.next().is_none()
        && !grouped_roots.contains(&(watched_folder_path.to_string(), first.to_string()))
    {
        return None;
    }
    let key = FolderReleaseDecisionKey {
        watched_folder_path: watched_folder_path.to_string(),
        relative_folder_path: first.to_string(),
    };
    Some(TriageGroup {
        key,
        name: first.to_string(),
    })
}

fn push_section_entry(
    sections: &mut Vec<TriageSection>,
    tab: TriageTab,
    watched_folder_path: &str,
    group: Option<TriageGroup>,
    entry: TriageEntry,
) {
    let group_path = group
        .as_ref()
        .map(|group| group.key.relative_folder_path.as_str());
    if let Some(section) = sections.iter_mut().find(|section| {
        section.tab == tab
            && section.watched_folder_path == watched_folder_path
            && section
                .group
                .as_ref()
                .map(|group| group.key.relative_folder_path.as_str())
                == group_path
    }) {
        section.entries.push(entry);
        return;
    }
    sections.push(TriageSection {
        tab,
        watched_folder_path: watched_folder_path.to_string(),
        group,
        entries: vec![entry],
    });
}

fn project_sections(
    watched_folders: &[super::folder_registry::WatchedFolder],
    rows: &[TriageRow],
    invalid: &[InvalidCandidate],
    boundaries: &[FolderReleaseBoundary],
) -> Vec<TriageSection> {
    struct OrderedEntry {
        watched_folder_path: String,
        display_path: String,
        tab: TriageTab,
        group: Option<TriageGroup>,
        entry: TriageEntry,
    }

    let mut grouped_roots = HashSet::new();
    let mut note_descendant = |watched_folder_path: &str, display_path: &str, hidden: bool| {
        let mut components = display_path
            .split('/')
            .filter(|component| !component.is_empty());
        if let Some(first) = components.next() {
            if hidden || components.next().is_some() {
                grouped_roots.insert((watched_folder_path.to_string(), first.to_string()));
            }
        }
    };
    for row in rows {
        note_descendant(&row.watched_folder_path, &row.display_path, false);
    }
    for candidate in invalid {
        note_descendant(
            &candidate.watched_folder_path,
            &candidate.display_path,
            false,
        );
    }
    for boundary in boundaries {
        note_descendant(
            &boundary.key.watched_folder_path,
            &boundary.display_path,
            !boundary.tree_rows.is_empty(),
        );
    }

    let mut ordered = Vec::with_capacity(rows.len() + invalid.len() + boundaries.len());
    for row in rows {
        ordered.push(OrderedEntry {
            watched_folder_path: row.watched_folder_path.clone(),
            display_path: row.display_path.clone(),
            tab: row.placement.tab(),
            group: group_for(&row.watched_folder_path, &row.display_path, &grouped_roots),
            entry: TriageEntry::Candidate(row.clone()),
        });
    }
    for boundary in boundaries {
        ordered.push(OrderedEntry {
            watched_folder_path: boundary.key.watched_folder_path.clone(),
            display_path: boundary.display_path.clone(),
            tab: TriageTab::NeedsYou,
            group: group_for(
                &boundary.key.watched_folder_path,
                &boundary.display_path,
                &grouped_roots,
            ),
            entry: TriageEntry::Boundary(boundary.clone()),
        });
    }
    for candidate in invalid {
        ordered.push(OrderedEntry {
            watched_folder_path: candidate.watched_folder_path.clone(),
            display_path: candidate.display_path.clone(),
            tab: TriageTab::Skipped,
            group: group_for(
                &candidate.watched_folder_path,
                &candidate.display_path,
                &grouped_roots,
            ),
            entry: TriageEntry::Invalid(candidate.clone()),
        });
    }
    let root_order: HashMap<_, _> = watched_folders
        .iter()
        .enumerate()
        .map(|(index, folder)| (folder.path.as_str(), index))
        .collect();
    ordered.sort_by(|left, right| {
        root_order
            .get(left.watched_folder_path.as_str())
            .cmp(&root_order.get(right.watched_folder_path.as_str()))
            .then_with(|| natord::compare(&left.display_path, &right.display_path))
    });

    let mut sections = Vec::new();
    for ordered in ordered {
        push_section_entry(
            &mut sections,
            ordered.tab,
            &ordered.watched_folder_path,
            ordered.group,
            ordered.entry,
        );
    }
    sections
}

/// Read the queue: the scanned candidates, their stored verdicts, and one live
/// library check across every match those verdicts name.
///
/// The library check is live and deliberately not stored with the verdict —
/// another import landing flips a candidate from Ready to "already in library"
/// without its own verdict changing.
pub async fn load(
    import: &ImportServiceHandle,
    library_manager: &LibraryManager,
) -> Result<TriageQueue, LibraryError> {
    let snapshot = import.get_import_candidates();
    let stored = library_manager.load_import_candidate_states().await?;
    let verdicts = stored_verdicts(&snapshot, &stored)?;

    // One check for the whole queue, deduplicated by release id — the key
    // `ready::classify` matches statuses against.
    let checks = checks_from_verdicts(&verdicts);
    let statuses = library_manager.check_releases_in_library(&checks).await?;
    let answers = answers_from_statuses(verdicts, &statuses)?;
    let picks = stored_picks(&snapshot, &stored, library_manager).await?;
    Ok(project(snapshot, &answers, &picks))
}

pub(crate) fn library_checks(
    snapshot: &ImportCandidatesSnapshot,
    stored: &HashMap<String, DbImportCandidateState>,
) -> Result<Vec<LibraryCheck>, LibraryError> {
    Ok(checks_from_verdicts(&stored_verdicts(snapshot, stored)?))
}

pub(crate) fn project_live(
    snapshot: ImportCandidatesSnapshot,
    projection: crate::db::ImportTriageDbProjection,
) -> Result<TriageQueue, LibraryError> {
    let verdicts = stored_verdicts(&snapshot, &projection.candidate_states)?;
    let answers = answers_from_statuses(verdicts, &projection.library_statuses)?;
    let picks = stored_picks_from_payloads(
        &snapshot,
        &projection.candidate_states,
        &projection.source_payloads,
    )?;
    Ok(project(snapshot, &answers, &picks))
}

fn checks_from_verdicts(
    verdicts: &HashMap<(String, u64), StoredCandidateVerdict>,
) -> Vec<LibraryCheck> {
    let mut seen = std::collections::HashSet::new();
    verdicts
        .values()
        .flat_map(|stored| named_matches(&stored.verdict))
        .filter(|result| seen.insert(result.release_id.clone()))
        .map(LibraryCheck::from)
        .collect()
}

fn answers_from_statuses(
    verdicts: HashMap<(String, u64), StoredCandidateVerdict>,
    statuses: &[LibraryStatus],
) -> Result<HashMap<(String, u64), Answered>, LibraryError> {
    let by_release: HashMap<&str, &LibraryStatus> = statuses
        .iter()
        .map(|status| (status.release_id.as_str(), status))
        .collect();

    let mut answers = HashMap::with_capacity(verdicts.len());
    for (candidate_identity, stored) in verdicts {
        let mut library_statuses = Vec::new();
        for result in named_matches(&stored.verdict) {
            // A release the check did not answer for is the one failure that
            // must not be absorbed. `ready::in_library` reads a missing status
            // as "not in the library", which is the difference between Needs
            // you and Ready — a record the user already owns, silently admitted
            // to a bulk import nobody looks at. "We do not know" is not "no", so
            // the whole read fails and the sidebar shows that instead of a
            // confident wrong answer.
            let status = by_release.get(result.release_id.as_str()).ok_or_else(|| {
                LibraryError::Internal(format!(
                    "the library check returned no status for release {}; the import queue \
                     cannot be classified against a partial answer",
                    result.release_id
                ))
            })?;
            library_statuses.push((*status).clone());
        }
        answers.insert(
            candidate_identity,
            Answered::new(
                stored.verdict,
                stored.probed_total_duration_ms,
                &library_statuses,
            ),
        );
    }

    Ok(answers)
}

/// The matches a verdict names as *the* answer — the ones the Ready rule checks
/// against the library. A `Conflict`'s two result sets are what the signals
/// each saw, not a match, so nothing is looked up for them.
fn named_matches(verdict: &TerminalVerdict) -> impl Iterator<Item = &MetadataResult> {
    let matches: &[MetadataResult] = match verdict {
        TerminalVerdict::Found { matches, .. } => matches,
        TerminalVerdict::Conflict { .. }
        | TerminalVerdict::NotFoundAnywhere
        | TerminalVerdict::ManualOnly { .. } => &[],
    };
    matches.iter()
}

/// The stored verdict and probed total for every scanned candidate that has
/// one, keyed by content hash. A row this build cannot decode fails the read
/// through [`decode_stored`]; persisted state is never silently omitted.
struct StoredCandidateVerdict {
    verdict: TerminalVerdict,
    probed_total_duration_ms: u64,
}

fn stored_verdicts(
    snapshot: &ImportCandidatesSnapshot,
    stored: &HashMap<String, DbImportCandidateState>,
) -> Result<HashMap<(String, u64), StoredCandidateVerdict>, LibraryError> {
    let mut out = HashMap::new();
    for candidate in &snapshot.folder_candidates {
        let content_hash = candidate.candidate.files.content_hash();
        let candidate_identity = (content_hash.clone(), candidate.candidate.file_edit_revision);
        if out.contains_key(&candidate_identity) {
            continue;
        }
        let Some(row) = stored.get(&content_hash) else {
            continue;
        };
        if row.file_edits.revision != candidate.candidate.file_edit_revision {
            continue;
        }
        let Some(verdict) = decode_stored(row).map_err(LibraryError::Internal)? else {
            continue;
        };
        // `decode_stored` returning a verdict means the row carries an identify
        // result, so this is present for the same reason.
        let identify = row
            .identify
            .as_ref()
            .expect("a decoded verdict came from an identify result");
        // Nothing that writes this column can produce a negative total.
        // Clamping one to zero would classify the candidate
        // `LocalDurationUnknown` — a plausible-looking answer standing in for a
        // corrupt row — so it is refused instead.
        let probed_total_duration_ms =
            u64::try_from(identify.probed_total_duration_ms).map_err(|_| {
                LibraryError::Internal(format!(
                "import_candidate_state row {content_hash} holds a negative probed total ({}ms)",
                identify.probed_total_duration_ms
            ))
            })?;
        out.insert(
            candidate_identity,
            StoredCandidateVerdict {
                verdict,
                probed_total_duration_ms,
            },
        );
    }
    Ok(out)
}

/// The identity pick every scanned candidate's stored row holds, with the
/// release each one settled on read back out of the documents behind it.
///
/// Keyed like the verdicts: by content hash and edit revision, so a read racing
/// an edit write serves nothing rather than mixed state. A pick this build
/// cannot decode fails the read; persisted state is never silently omitted.
async fn stored_picks(
    snapshot: &ImportCandidatesSnapshot,
    stored: &HashMap<String, DbImportCandidateState>,
    library_manager: &LibraryManager,
) -> Result<HashMap<(String, u64), Picked>, LibraryError> {
    let mut out: HashMap<(String, u64), Picked> = HashMap::new();
    for candidate in &snapshot.folder_candidates {
        let content_hash = candidate.candidate.files.content_hash();
        let candidate_identity = (content_hash.clone(), candidate.candidate.file_edit_revision);
        if out.contains_key(&candidate_identity) {
            continue;
        }
        let Some(row) = stored.get(&content_hash) else {
            continue;
        };
        if row.file_edits.revision != candidate.candidate.file_edit_revision {
            continue;
        }
        let Some(pick_json) = row.identity_pick.as_ref() else {
            continue;
        };
        let pick: crate::import::IdentityPick =
            serde_json::from_str(pick_json).map_err(|error| {
                LibraryError::Internal(format!(
                    "stored identity pick for {content_hash} does not decode: {error}"
                ))
            })?;
        let release = picked_release(library_manager, &pick).await?;
        out.insert(candidate_identity, Picked { pick, release });
    }
    Ok(out)
}

fn stored_picks_from_payloads(
    snapshot: &ImportCandidatesSnapshot,
    stored: &HashMap<String, DbImportCandidateState>,
    payloads: &HashMap<(crate::import::PayloadSource, String), String>,
) -> Result<HashMap<(String, u64), Picked>, LibraryError> {
    let mut out = HashMap::new();
    for candidate in &snapshot.folder_candidates {
        let content_hash = candidate.candidate.files.content_hash();
        let candidate_identity = (content_hash.clone(), candidate.candidate.file_edit_revision);
        if out.contains_key(&candidate_identity) {
            continue;
        }
        let Some(row) = stored.get(&content_hash) else {
            continue;
        };
        if row.file_edits.revision != candidate.candidate.file_edit_revision {
            continue;
        }
        let Some(pick_json) = row.identity_pick.as_ref() else {
            continue;
        };
        let pick: crate::import::IdentityPick =
            serde_json::from_str(pick_json).map_err(|error| {
                LibraryError::Internal(format!(
                    "stored identity pick for {content_hash} does not decode: {error}"
                ))
            })?;
        let release = picked_release_from_payloads(&pick, payloads)?;
        out.insert(candidate_identity, Picked { pick, release });
    }
    Ok(out)
}

fn picked_release_from_payloads(
    pick: &crate::import::IdentityPick,
    payloads: &HashMap<(crate::import::PayloadSource, String), String>,
) -> Result<Option<MatchedRelease>, LibraryError> {
    let crate::import::IdentityPick::Release {
        source, release_id, ..
    } = pick
    else {
        return Ok(None);
    };
    let release = crate::import::MetadataRef::new(release_id.clone(), *source);
    let Some(payloads) = crate::import::payloads::load_from_map(&release, payloads)
        .map_err(|error| LibraryError::Internal(error.to_string()))?
    else {
        return Ok(None);
    };
    let detail = payloads
        .detail()
        .map_err(|error| LibraryError::Internal(error.to_string()))?;
    Ok(Some(MatchedRelease::of_pick(*source, &detail)))
}

/// The release a pick settled on, as the documents archived for it describe it.
///
/// Reading the folder as its own tags settles it on no release, and so leads
/// with nothing. Neither does a release pick whose documents are not archived:
/// the pick is persisted before they are fetched, so the window exists, and the
/// pane's own read is what closes it.
async fn picked_release(
    library_manager: &LibraryManager,
    pick: &crate::import::IdentityPick,
) -> Result<Option<MatchedRelease>, LibraryError> {
    let crate::import::IdentityPick::Release {
        source, release_id, ..
    } = pick
    else {
        return Ok(None);
    };
    let release = crate::import::MetadataRef::new(release_id.clone(), *source);
    let payloads = library_manager
        .load_release_payloads(&release)
        .await
        .map_err(|error| LibraryError::Internal(error.to_string()))?;
    let Some(payloads) = payloads else {
        tracing::debug!(
            "nothing archived the documents behind picked {} release {release_id}; its row leads \
             with the folder name until they are fetched",
            source.as_str()
        );
        return Ok(None);
    };
    let detail = payloads
        .detail()
        .map_err(|error| LibraryError::Internal(error.to_string()))?;
    Ok(Some(MatchedRelease::of_pick(*source, &detail)))
}

#[cfg(test)]
mod tests;
