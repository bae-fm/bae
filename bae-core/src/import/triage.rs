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

use super::folder_scanner::{FolderCandidate, InvalidCandidate};
use super::handle::{
    CandidateImportStatusSnapshot, FolderImportCandidateSnapshot, ImportCandidatesSnapshot,
    ImportServiceHandle,
};
use super::search::{MetadataResult, SourceTracks};
use super::types::MetadataSource;
use crate::db::{DbImportCandidateState, LibraryCheck, LibraryStatus};
use crate::identify::verdict::decode_stored;
use crate::identify::{
    classify, IdentifyState, NeedsYou, QueueClassification, ResultProvenance, TerminalVerdict,
};
use crate::library::{LibraryError, LibraryManager};
use std::collections::HashMap;

/// The sidebar's four tabs, split by what the queue needs from the user rather
/// than by lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriageTab {
    Ready,
    NeedsYou,
    Done,
    Skipped,
}

/// Where a row sits: its tab, and — under Needs you — why it is there.
///
/// One field rather than a tab plus an optional group, so "Needs you with no
/// group" and "Ready carrying a group" are not states anything can represent.
/// See `many-fields-none-together-means-a-missing-type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriagePlacement {
    /// Exactly one match, not in the library, counts and lengths agree — safe
    /// to import unattended.
    Ready,
    NeedsYou {
        /// The question this row batches under. Derived from `reason` at
        /// construction and never independently: group 3 collapses four
        /// classification variants into one question, so the two must not be
        /// able to disagree.
        group: NeedsYouGroup,
        /// The classification's own variant, kept whole so the row can name the
        /// disagreement precisely even when its group cannot.
        reason: NeedsYouReason,
    },
    Done,
    Skipped,
}

impl TriagePlacement {
    pub fn tab(&self) -> TriageTab {
        match self {
            Self::Ready => TriageTab::Ready,
            Self::NeedsYou { .. } => TriageTab::NeedsYou,
            Self::Done => TriageTab::Done,
            Self::Skipped => TriageTab::Skipped,
        }
    }
}

/// The Needs-you groups, in the order the sidebar stacks them. Declaration
/// order *is* display order — [`NeedsYouGroup::IN_ORDER`] hands it to a surface
/// so neither UI invents its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NeedsYouGroup {
    /// Several pressings matched.
    PickAPressing,
    /// The disc ID and the barcode point at different releases.
    SignalsDisagree,
    /// The folder and the source disagree about what is here. Four
    /// classification variants share this group because they are one question
    /// to the user; the row's `reason` still names which.
    CountsOrLengthsDisagree,
    AlreadyInLibrary,
    /// Nothing matched, or there was nothing to look up.
    NoMatch,
    /// No verdict yet.
    StillIdentifying,
}

impl NeedsYouGroup {
    pub const IN_ORDER: [Self; 6] = [
        Self::PickAPressing,
        Self::SignalsDisagree,
        Self::CountsOrLengthsDisagree,
        Self::AlreadyInLibrary,
        Self::NoMatch,
        Self::StillIdentifying,
    ];

    /// The group a reason batches under. The one place the collapse happens.
    pub fn of(reason: &NeedsYouReason) -> Self {
        let needs_you = match reason {
            NeedsYouReason::StillIdentifying { .. } => return Self::StillIdentifying,
            NeedsYouReason::Disagreement(needs_you) => needs_you,
        };
        match needs_you {
            NeedsYou::SeveralMatches { .. } => Self::PickAPressing,
            NeedsYou::SignalsConflict => Self::SignalsDisagree,
            NeedsYou::TrackCountDisagrees { .. }
            | NeedsYou::DurationsDisagree { .. }
            | NeedsYou::SourceLengthsUnknown
            | NeedsYou::LocalDurationUnknown => Self::CountsOrLengthsDisagree,
            NeedsYou::AlreadyInLibrary => Self::AlreadyInLibrary,
            NeedsYou::NoMatch | NeedsYou::NothingToLookUp => Self::NoMatch,
        }
    }
}

/// Why a row needs you.
///
/// [`NeedsYou`] rides along whole rather than being re-spelled here — it
/// already carries every operand a row states its disagreement with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeedsYouReason {
    /// The stored verdict classified to this.
    Disagreement(NeedsYou),
    /// No stored verdict yet. Work in progress, not a decision anyone is being
    /// asked for — and `phase` says *which* kind of work in progress, because
    /// three unlike states share this group.
    StillIdentifying { phase: IdentifyPhase },
}

/// How far identification has got for a candidate with no stored verdict.
///
/// Without this the row cannot tell three different things apart, and the
/// design's dimmed group is supposed to show which: a candidate the sweep has
/// not reached, one being worked on right now, and one whose run *finished* but
/// produced nothing storable. The third is not rare —
/// [`TerminalVerdict::try_from`] refuses any terminal state carrying a recorded
/// lookup failure, so every network blip lands there — and rendering it as
/// "working on it" would promise progress that nothing is making.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifyPhase {
    /// Nothing has run yet: the sweep has not reached this candidate.
    Queued,
    /// A run is in flight — signals are being gathered, or a lookup is out.
    Running,
    /// A run settled without an answer worth keeping: a lookup that never
    /// responded, so half the evidence is missing and nothing was stored. It is
    /// retried on a later pass; nobody is waiting on this one.
    ///
    /// A verdict that has just been accepted and is still being written reads
    /// this way for the moment between the two — which is the same shape as the
    /// answer arriving a moment later.
    NoAnswer,
}

impl IdentifyPhase {
    /// Read the phase off the candidate's live identify state. Only meaningful
    /// when no verdict is stored: with one stored, the classification is the
    /// answer and the phase says nothing.
    pub fn of(state: &IdentifyState) -> Self {
        match state {
            IdentifyState::Idle => Self::Queued,
            IdentifyState::Triangulating { .. } => Self::Running,
            IdentifyState::Found { .. }
            | IdentifyState::Conflict { .. }
            | IdentifyState::NotFoundAnywhere { .. }
            | IdentifyState::ManualOnly { .. } => Self::NoAnswer,
        }
    }
}

/// What is known about one candidate: the classification of its stored verdict,
/// or — with no verdict — how far identification has got.
///
/// One value rather than an `Option` beside a phase field, because the phase is
/// meaningless once a verdict exists and a caller should not be able to hand
/// over both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateAnswer {
    Classified(QueueClassification),
    Unanswered(IdentifyPhase),
}

/// Which signal produced a match — the row's trailing evidence chip, and the
/// confidence cue the design leans on.
///
/// Two variants because `combine` builds a `Found`'s matches out of exactly two
/// result sets. The design's third chip ("matched on text") has no producer
/// today; it arrives with the code that searches by text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchedSignal {
    /// The disc's table of contents. Named ahead of the barcode when both
    /// lookups returned the release: a disc ID identifies the pressing, a
    /// barcode only the product.
    DiscId,
    Barcode,
}

impl MatchedSignal {
    /// `None` when neither lookup claims the result — the row then shows the
    /// provider alone. `combine` takes every match from one of the two result
    /// sets, so this does not arise from its output; the booleans are
    /// independent in the type, and naming a signal for a provenance that
    /// claims none would be worse than admitting there isn't one.
    fn of(provenance: &ResultProvenance) -> Option<Self> {
        if provenance.by_disc_id {
            Some(Self::DiscId)
        } else if provenance.by_barcode {
            Some(Self::Barcode)
        } else {
            None
        }
    }
}

/// Which provider answered, and what matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchEvidence {
    pub source: MetadataSource,
    pub signal: Option<MatchedSignal>,
}

/// The pressing-level facts about a match — the ones that differ between the
/// editions of one album.
///
/// Present as a whole exactly when the pressing is settled, which is when the
/// verdict named one match. With several in play the row is *asking* which
/// pressing, so none of these is known, and absent-together is then a state
/// rather than a convention three separate `Option`s would leave a consumer to
/// honour. See `many-fields-none-together-means-a-missing-type`; `db::Pressing`
/// is the same shape for the same reason.
///
/// The fields stay optional inside it: a settled pressing may well state a year
/// and no format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedPressing {
    pub year: Option<i32>,
    pub format: Option<String>,
    /// What the source says this release holds, once something asked. `None`
    /// when nobody has, or when the source answered and listed nothing.
    pub track_count: Option<u32>,
}

/// The release a row leads with. Absent as a whole when nothing matched — the
/// row then has the folder name as its title and no metadata line at all, so a
/// surface cannot render a half-populated match.
///
/// Populated for Done and Skipped rows too, deliberately: a candidate that has
/// been imported or set aside still shows what it was matched to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedRelease {
    /// The lead match's title. Titles vary between the editions of a release
    /// group, so with several matches this is one pressing's title standing in
    /// for the album — see [`MatchedRelease::of`].
    pub title: String,
    /// The lead match's artist, with the same caveat as `title`.
    pub artist: Option<String>,
    /// The facts that are only known once the pressing is settled.
    pub pressing: Option<MatchedPressing>,
    /// [`crate::import::cover_art::RemoteCover::thumbnail_url`] of the lead
    /// match — a row renders a 40px cover, and the full-size URL is the mapping
    /// pane's business. Cover art is fetched per release id, so this is that one
    /// pressing's sleeve, not the group's.
    pub cover_thumbnail_url: Option<String>,
    pub evidence: MatchEvidence,
}

impl MatchedRelease {
    /// The release a verdict leads with, or `None` when it named none.
    ///
    /// `Conflict`, `NotFoundAnywhere` and `ManualOnly` all lead with nothing:
    /// the first has results but no agreement on which is the match, and the
    /// other two have no results at all.
    ///
    /// With several matches the row still leads with the first one's title,
    /// artist and cover. Those are not group-level truths — a release group
    /// spans remasters and reissues that differ in all three — they are the
    /// lead pressing's, standing in for the album until someone picks. What is
    /// *not* shown is `pressing`: year, format and track count are the question
    /// being asked, and answering it from the first candidate would be the app
    /// pre-empting the user.
    fn of(verdict: &TerminalVerdict) -> Option<Self> {
        let TerminalVerdict::Found {
            matches,
            provenance,
            ..
        } = verdict
        else {
            return None;
        };
        let lead = matches.first()?;
        let settled = matches.len() == 1;
        Some(Self {
            title: lead.title.clone(),
            artist: lead.artist.clone(),
            pressing: settled.then(|| MatchedPressing {
                year: lead.year,
                format: lead.format.clone(),
                track_count: source_track_count(lead),
            }),
            cover_thumbnail_url: lead.cover_art.as_ref().map(|c| c.thumbnail_url.clone()),
            evidence: MatchEvidence {
                source: lead.source,
                // Index-aligned with `matches`, so the lead's provenance is the
                // first one.
                signal: provenance.first().and_then(MatchedSignal::of),
            },
        })
    }
}

fn source_track_count(result: &MetadataResult) -> Option<u32> {
    match result.source_tracks {
        Some(SourceTracks::Listed { count, .. }) => Some(count),
        Some(SourceTracks::Nothing) | None => None,
    }
}

/// One candidate's row.
#[derive(Debug, Clone)]
pub struct TriageRow {
    /// The candidate's folder path — the key every other import call takes.
    pub candidate_key: String,
    /// The folder on disk: the mono subtitle, and the row's title when nothing
    /// matched.
    pub folder_name: String,
    /// The watched folder this candidate was scanned from — the sidebar's
    /// existing section key. Match it against `WatchedFolder::path`.
    pub watched_folder_path: String,
    pub placement: TriagePlacement,
    /// The release the row leads with. `None` and the folder name is the title.
    pub matched: Option<MatchedRelease>,
    /// Whether this row takes a bulk-import checkbox — exactly the Ready rows,
    /// which is the whole point of the Ready rule. Carried rather than left to
    /// each UI so the rule is stated once.
    pub selectable: bool,
    pub import_status: Option<CandidateImportStatusSnapshot>,
}

/// How many rows each tab holds. Computed in the same pass that places them, so
/// the number on a tab and the rows behind it cannot drift, and neither UI
/// counts an array length.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TriageTabCounts {
    pub ready: u32,
    pub needs_you: u32,
    pub done: u32,
    pub skipped: u32,
}

impl TriageTabCounts {
    fn bump(&mut self, tab: TriageTab) {
        match tab {
            TriageTab::Ready => self.ready += 1,
            TriageTab::NeedsYou => self.needs_you += 1,
            TriageTab::Done => self.done += 1,
            TriageTab::Skipped => self.skipped += 1,
        }
    }
}

/// The whole sidebar: every candidate's row, the invalid folders that share the
/// Skipped tab with them, and what each tab reads.
#[derive(Debug, Clone)]
pub struct TriageQueue {
    /// Ordered by watched folder — in the watched-folder list's own order —
    /// then by candidate key. That is the scan snapshot's order, kept: sorting
    /// is the store's and grouping is `placement`'s.
    pub rows: Vec<TriageRow>,
    /// Folders that looked like releases and failed validation. They are not
    /// rows — no verdict, no import, nothing to triage — but they live in the
    /// Skipped tab, so they travel with the counts rather than being fetched
    /// separately: two reads at two instants can disagree about how many there
    /// are, and then the tab's number contradicts its own list.
    pub invalid: Vec<InvalidCandidate>,
    /// `skipped` counts the Skipped rows **plus** `invalid`.
    pub counts: TriageTabCounts,
}

/// What a candidate's stored row says, read back.
///
/// The verdict and its classification travel as one value because they are the
/// same fact read two ways — the row's release comes from the verdict and its
/// placement from the classification — and pairing them here is what stops a
/// caller matching a verdict against somebody else's classification.
#[derive(Debug, Clone)]
pub struct Answered {
    pub verdict: TerminalVerdict,
    pub classification: QueueClassification,
}

impl Answered {
    /// Classify a stored verdict. `library_statuses` is a **live** check of the
    /// verdict's own matches — see [`classify`], which is why nothing here is
    /// cached.
    pub fn new(
        verdict: TerminalVerdict,
        probed_total_duration_ms: u64,
        library_statuses: &[LibraryStatus],
    ) -> Self {
        let classification = classify(&verdict, probed_total_duration_ms, library_statuses);
        Self {
            verdict,
            classification,
        }
    }
}

/// Which tab a candidate belongs to, and — under Needs you — why.
///
/// A total function of four facts core already holds, checked in one order:
///
/// 1. **Done outranks everything.** A candidate that has been imported, is
///    being imported, or failed importing is not awaiting triage, whatever its
///    verdict says and whether or not it was ever skipped.
/// 2. **Then Skipped**, which is a decision the user already made.
/// 3. **Then what is known about it**, which is the only thing that can put a
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
    // Spelled out rather than `is_some()`: every variant of an import status
    // means the import began, and a new one should have to be placed here on
    // purpose rather than inherited by an `_`.
    let import_started = match import_status {
        Some(
            CandidateImportStatusSnapshot::Importing { .. }
            | CandidateImportStatusSnapshot::Complete { .. }
            | CandidateImportStatusSnapshot::Error { .. },
        ) => true,
        None => false,
    };
    if is_added || import_started {
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
fn row(snapshot: &FolderImportCandidateSnapshot, answer: Option<&Answered>) -> TriageRow {
    let FolderCandidate {
        path,
        name,
        watched_folder_path,
        skipped,
        is_added,
        files: _,
    } = &snapshot.candidate;
    let import_status = snapshot.runtime.import_status.clone();
    let known = match answer {
        Some(answer) => CandidateAnswer::Classified(answer.classification.clone()),
        None => CandidateAnswer::Unanswered(IdentifyPhase::of(&snapshot.runtime.identify_state)),
    };
    let placement = place(*skipped, *is_added, import_status.as_ref(), &known);
    TriageRow {
        candidate_key: path.to_string_lossy().into_owned(),
        folder_name: name.clone(),
        watched_folder_path: watched_folder_path.clone(),
        selectable: matches!(placement, TriagePlacement::Ready),
        matched: answer.and_then(|answer| MatchedRelease::of(&answer.verdict)),
        placement,
        import_status,
    }
}

/// Every candidate's row and the four tab counts, in one pass.
///
/// `answers` is keyed by content hash, which is what the stored rows are keyed
/// by; a candidate with no entry has no verdict yet.
pub fn project(
    snapshot: ImportCandidatesSnapshot,
    answers: &HashMap<String, Answered>,
) -> TriageQueue {
    let ImportCandidatesSnapshot {
        folder_candidates,
        invalid_candidates,
        // The section headers are the sidebar's own query; a row names its
        // watched folder by path.
        watched_folders: _,
    } = snapshot;

    let mut rows = Vec::with_capacity(folder_candidates.len());
    let mut counts = TriageTabCounts {
        skipped: invalid_candidates.len() as u32,
        ..TriageTabCounts::default()
    };
    for candidate in &folder_candidates {
        let row = row(
            candidate,
            answers.get(&candidate.candidate.files.content_hash()),
        );
        counts.bump(row.placement.tab());
        rows.push(row);
    }
    TriageQueue {
        rows,
        invalid: invalid_candidates,
        counts,
    }
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
    let checks: Vec<LibraryCheck> = {
        let mut seen = std::collections::HashSet::new();
        verdicts
            .values()
            .flat_map(|(verdict, _)| named_matches(verdict))
            .filter(|result| seen.insert(result.release_id.clone()))
            .map(LibraryCheck::from)
            .collect()
    };
    let statuses = library_manager.check_releases_in_library(&checks).await?;
    let by_release: HashMap<&str, &LibraryStatus> = statuses
        .iter()
        .map(|status| (status.release_id.as_str(), status))
        .collect();

    let mut answers = HashMap::with_capacity(verdicts.len());
    for (content_hash, (verdict, probed_total_duration_ms)) in verdicts {
        let mut library_statuses = Vec::new();
        for result in named_matches(&verdict) {
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
            content_hash,
            Answered::new(verdict, probed_total_duration_ms, &library_statuses),
        );
    }

    Ok(project(snapshot, &answers))
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
/// one, keyed by content hash. A row this build cannot decode is absent — see
/// [`decode_stored`], which is where that rule lives for the sweep too.
fn stored_verdicts(
    snapshot: &ImportCandidatesSnapshot,
    stored: &HashMap<String, DbImportCandidateState>,
) -> Result<HashMap<String, (TerminalVerdict, u64)>, LibraryError> {
    let mut out = HashMap::new();
    for candidate in &snapshot.folder_candidates {
        let content_hash = candidate.candidate.files.content_hash();
        if out.contains_key(&content_hash) {
            continue;
        }
        let Some(row) = stored.get(&content_hash) else {
            continue;
        };
        let Some(verdict) = decode_stored(row) else {
            continue;
        };
        // Nothing that writes this column can produce a negative total.
        // Clamping one to zero would classify the candidate
        // `LocalDurationUnknown` — a plausible-looking answer standing in for a
        // corrupt row — so it is refused instead.
        let probed_total_duration_ms =
            u64::try_from(row.probed_total_duration_ms).map_err(|_| {
                LibraryError::Internal(format!(
                "import_candidate_state row {content_hash} holds a negative probed total ({}ms)",
                row.probed_total_duration_ms
            ))
            })?;
        out.insert(content_hash, (verdict, probed_total_duration_ms));
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
