//! Placement columns in, an ordered list of item references out.
//!
//! One pass over the queue answers every question the tab asks: where each
//! entry sits, which group it joins, which tab it counts against, whether the
//! filter keeps it, and — for the chrome — the Ready set, the group keys and
//! the row the identify count is still waiting on. Nothing here reads a file,
//! a cue sheet, a boundary tree or an archived document; those are loaded for
//! the items inside the requested windows and nowhere else.

use super::{
    GroupHeaderRow, ImportListItem, ImportListOrder, ImportListRequest, ImportListView,
    ImportQueueSummary, PlacedRow, ReadyRowRef, UploadStanding,
};
use crate::db::{ImportQueueRows, ScanCandidateKind, ScanCandidateListRow};
use crate::identify::classify_summary;
use crate::import::folder_registry::candidate_relative_path;
use crate::import::triage::{
    import_status_of, place, CandidateAnswer, MatchedRelease, TriageGroup, TriageImportStatus,
    TriagePlacement, TriageRow, TriageRuntimeFacts, TriageTab, TriageTabCounts,
};
use crate::import::types::IdentityPick;
use crate::import::FolderReleaseDecisionKey;
use crate::library::LibraryError;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

/// Where one item in the list comes from. The windows resolve these; the
/// entries outside them are never built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemRef {
    /// Into [`Flattened::headers`].
    Header(usize),
    /// Into [`Flattened::rows`].
    Candidate(usize),
    /// Into [`ImportQueueRows::candidates`] — a row the scan found invalid.
    Invalid(usize),
}

/// The whole queue, flattened for one view.
pub(crate) struct Flattened {
    pub(crate) items: Vec<ItemRef>,
    pub(crate) headers: Vec<GroupHeaderRow>,
    pub(crate) rows: Vec<PlacedRow>,
    pub(crate) summary: ImportQueueSummary,
}

/// One entry of the queue before the tab filter and the grouping runs.
struct OrderedEntry {
    watched_folder_path: String,
    display_path: String,
    tab: TriageTab,
    group: Option<TriageGroup>,
    matches_filter: bool,
    item: ItemRef,
    /// How a Done row sorts against its neighbours. `None` on every other tab,
    /// which orders by path instead.
    done_order: Option<DoneOrder>,
}

/// The Done tab's order: what the cloud is still doing with the release, then
/// when it was imported, newest first.
///
/// Not the path: a folder that is in the library is finished, and what a person
/// looks for there is the import they just ran and whatever is still going up
/// behind it — neither of which the alphabet answers.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct DoneOrder {
    upload_rank: u8,
    /// Reversed so the newest import leads. A release with no recorded time
    /// sorts last rather than first.
    imported_at: std::cmp::Reverse<Option<i64>>,
}

pub(crate) fn flatten(
    rows: &ImportQueueRows,
    request: &ImportListRequest,
) -> Result<Flattened, LibraryError> {
    let view = &request.view;
    let runtime_facts = &request.runtime_facts;
    let idle = TriageRuntimeFacts::default();
    let mut placed = Vec::new();
    let mut counts = TriageTabCounts::default();
    let mut ordered = Vec::with_capacity(rows.candidates.len());

    for (index, row) in rows.candidates.iter().enumerate() {
        match row.kind {
            ScanCandidateKind::Invalid => {
                counts.skipped += 1;
                ordered.push(OrderedEntry {
                    watched_folder_path: row.watched_folder_path.clone(),
                    display_path: row.display_path.clone(),
                    tab: TriageTab::Skipped,
                    group: None,
                    matches_filter: matches_text(
                        view,
                        [row.name.as_str(), row.display_path.as_str()],
                    ),
                    item: ItemRef::Invalid(index),
                    done_order: None,
                });
            }
            // A tentative candidate is a release approximation the scan found
            // before it knew what enclosed it. It is not a row, is not
            // counted, and does not make its first path component a group:
            // nothing can be asked of it until a later scan item settles what
            // it belongs to.
            ScanCandidateKind::Tentative => {}
            ScanCandidateKind::Valid => {
                let facts = runtime_facts.get(&row.path).unwrap_or(&idle);
                let triage_row = place_row(rows, row, facts)?;
                let tab = triage_row.placement.tab();
                counts.bump(tab);
                let matched = triage_row.matched.as_ref();
                let matches_filter = matches_text(
                    view,
                    [
                        row.name.as_str(),
                        row.display_path.as_str(),
                        matched.map_or("", |matched| matched.title.as_str()),
                        matched
                            .and_then(|matched| matched.artist.as_deref())
                            .unwrap_or(""),
                    ],
                );
                ordered.push(OrderedEntry {
                    watched_folder_path: row.watched_folder_path.clone(),
                    display_path: row.display_path.clone(),
                    tab,
                    group: None,
                    matches_filter,
                    item: ItemRef::Candidate(placed.len()),
                    done_order: (tab == TriageTab::Done)
                        .then(|| done_order(rows, row, &triage_row, &request.upload_standing)),
                });
                placed.push(PlacedRow {
                    row: triage_row,
                    index,
                });
            }
        }
    }

    let grouped_roots = grouped_roots(rows);
    let combinable_roots = combinable_roots(rows);
    for entry in &mut ordered {
        // A group header asks how the folder under it is read, and offers to
        // read it the other way. Both are questions about a folder nobody has
        // imported yet, so only a Pending row joins one; Done and Skipped are
        // flat lists of releases whose reading is settled.
        if entry.tab != TriageTab::Pending {
            continue;
        }
        entry.group = group_for(
            &entry.watched_folder_path,
            &entry.display_path,
            &grouped_roots,
            &combinable_roots,
        );
    }

    let root_order: HashMap<&str, usize> = rows
        .watched_folders
        .iter()
        .enumerate()
        .map(|(index, folder)| (folder.path.as_str(), index))
        .collect();
    // Sort every tab's entries in one pass, tab first so each tab's run is
    // contiguous and its own order is decided among its own rows. Which tab is
    // being shown is the filter's business, further down.
    ordered.sort_by(|left, right| {
        tab_rank(left.tab).cmp(&tab_rank(right.tab)).then_with(|| {
            match (&left.done_order, &right.done_order) {
                (Some(left_done), Some(right_done)) => left_done.cmp(right_done),
                _ => {
                    let by_root = root_order
                        .get(left.watched_folder_path.as_str())
                        .cmp(&root_order.get(right.watched_folder_path.as_str()));
                    let by_path =
                        natord::compare_ignore_case(&left.display_path, &right.display_path);
                    let natural = by_root.then(by_path);
                    match view.order {
                        ImportListOrder::PathAscending => natural,
                        ImportListOrder::PathDescending => natural.reverse(),
                    }
                }
            }
        })
    });

    let summary = summarise(rows, &ordered, &placed, counts);
    let (items, headers) = emit(view, &ordered);
    Ok(Flattened {
        items,
        headers,
        rows: placed,
        summary,
    })
}

/// One settled candidate's row, as the tables place it with this key's
/// runtime. `resolved_boundaries` is left empty and `matched` is the verdict's
/// lead — the window fills both in for the items it materialises.
fn place_row(
    rows: &ImportQueueRows,
    row: &ScanCandidateListRow,
    facts: &TriageRuntimeFacts,
) -> Result<TriageRow, LibraryError> {
    let content_hash = row.content_hash.as_deref().ok_or_else(|| {
        LibraryError::Internal(format!(
            "scanned candidate {} states no content hash",
            row.path
        ))
    })?;
    let state = rows
        .states
        .get(content_hash)
        .filter(|state| state.edit_revision == row.file_edit_revision);
    let verdict = state.and_then(|state| state.verdict.as_ref());
    let imported = rows.imported.get(content_hash);
    let import_status = import_status_of(
        facts.importing,
        imported,
        rows.failures.get(content_hash).map(String::as_str),
    );
    let answer = verdict.map(|verdict| {
        let lead_status = verdict
            .lead
            .as_ref()
            .and_then(|lead| rows.lead_statuses.get(&lead.release_id));
        classify_summary(
            verdict,
            state.map_or(0, |state| state.probed_total_duration_ms),
            lead_status,
        )
    });
    let known = match answer {
        Some(classification) => CandidateAnswer::Classified(classification),
        None => CandidateAnswer::Unanswered(facts.phase),
    };
    let skipped = rows.skipped.contains(&(
        row.watched_folder_path.clone(),
        candidate_relative_path(&row.watched_folder_path, Path::new(&row.path))
            .map_err(|error| LibraryError::Internal(error.to_string()))?,
    ));
    let picked = state.and_then(|state| state.pick.clone());
    let placement = place(
        skipped,
        imported.is_some(),
        import_status.as_ref(),
        picked.as_ref(),
        &known,
    );
    Ok(TriageRow {
        candidate_key: row.path.clone(),
        folder_name: row.name.clone(),
        watched_folder_path: row.watched_folder_path.clone(),
        display_path: row.display_path.clone(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: row.combine_ancestor_relative_path.clone().map(|relative| {
            FolderReleaseDecisionKey {
                watched_folder_path: row.watched_folder_path.clone(),
                relative_folder_path: relative,
            }
        }),
        // Every row the list holds is a settled release: a tentative
        // candidate never becomes one.
        actionable: true,
        skip_action: placement.skip_action(),
        selectable: matches!(placement, TriagePlacement::Ready),
        matched: verdict.and_then(MatchedRelease::of_summary),
        placement,
        import_status,
        claim: picked.as_ref().map(IdentityPick::choice),
        picked,
    })
}

/// Which run of the sorted vector a tab's entries form. Only the grouping
/// matters — each tab is filtered out on its own — but a stable one keeps the
/// comparator a total order.
fn tab_rank(tab: TriageTab) -> u8 {
    match tab {
        TriageTab::Pending => 0,
        TriageTab::Done => 1,
        TriageTab::Skipped => 2,
    }
}

/// Where one Done row sorts: what the cloud is still doing with the release it
/// became, then when that import happened.
fn done_order(
    rows: &ImportQueueRows,
    row: &ScanCandidateListRow,
    triage_row: &TriageRow,
    upload_standing: &BTreeMap<String, UploadStanding>,
) -> DoneOrder {
    let release_id = match &triage_row.import_status {
        Some(TriageImportStatus::Complete { release }) => Some(release.release_id.as_str()),
        _ => None,
    };
    DoneOrder {
        upload_rank: UploadStanding::rank(
            release_id.and_then(|id| upload_standing.get(id).copied()),
        ),
        imported_at: std::cmp::Reverse(
            row.content_hash
                .as_deref()
                .and_then(|hash| rows.imported_at.get(hash).copied()),
        ),
    }
}

/// The first path components that hold more than a flat row — a folder with a
/// nested candidate below it, or a boundary that has a tree. Rows under one of
/// those group; a row that is the only thing at its root does not.
fn grouped_roots(rows: &ImportQueueRows) -> HashSet<(String, String)> {
    let mut grouped = HashSet::new();
    let mut note = |watched_folder_path: &str, display_path: &str, hidden: bool| {
        let mut components = display_path
            .split('/')
            .filter(|component| !component.is_empty());
        if let Some(first) = components.next() {
            if hidden || components.next().is_some() {
                grouped.insert((watched_folder_path.to_string(), first.to_string()));
            }
        }
    };
    for row in &rows.candidates {
        if matches!(row.kind, ScanCandidateKind::Tentative) {
            continue;
        }
        note(&row.watched_folder_path, &row.display_path, false);
    }
    grouped
}

/// The folders the list can offer to read as one release: those settled as
/// several, and those holding several rows that nothing has settled either way.
/// Both are folders whose rows below could be one release; the header for such
/// a folder is where that is asked.
fn combinable_roots(rows: &ImportQueueRows) -> HashSet<(String, String)> {
    let mut combinable = rows.separated_folders.clone();
    for row in &rows.candidates {
        if let Some(relative) = &row.combine_ancestor_relative_path {
            combinable.insert((row.watched_folder_path.clone(), relative.clone()));
        }
    }
    combinable
}

fn group_for(
    watched_folder_path: &str,
    display_path: &str,
    grouped_roots: &HashSet<(String, String)>,
    combinable_roots: &HashSet<(String, String)>,
) -> Option<TriageGroup> {
    let mut components = display_path
        .split('/')
        .filter(|component| !component.is_empty());
    let first = components.next()?;
    let key = (watched_folder_path.to_string(), first.to_string());
    if components.next().is_none() && !grouped_roots.contains(&key) {
        return None;
    }
    Some(TriageGroup {
        combinable: combinable_roots.contains(&key),
        key: FolderReleaseDecisionKey {
            watched_folder_path: watched_folder_path.to_string(),
            relative_folder_path: first.to_string(),
        },
        name: first.to_string(),
    })
}

/// Whether one entry survives the view's filter. An empty filter keeps
/// everything.
fn matches_text<'a>(view: &ImportListView, haystack: impl IntoIterator<Item = &'a str>) -> bool {
    if view.filter_text.is_empty() {
        return true;
    }
    let needle = view.filter_text.to_lowercase();
    haystack
        .into_iter()
        .any(|value| value.to_lowercase().contains(&needle))
}

/// The chrome, over the whole queue rather than the requested tab: the counts
/// every tab bar shows, the Ready set the foot bar acts on, every group key
/// disclosure state is retained against, and the row the identify count is
/// still waiting on.
fn summarise(
    rows: &ImportQueueRows,
    ordered: &[OrderedEntry],
    placed: &[PlacedRow],
    counts: TriageTabCounts,
) -> ImportQueueSummary {
    let mut group_keys = Vec::new();
    let mut seen_groups = HashSet::new();
    let mut ready = Vec::new();
    let mut first_unidentified_key = None;
    for entry in ordered {
        if let Some(group) = &entry.group {
            if seen_groups.insert(group.key.clone()) {
                group_keys.push(group.key.clone());
            }
        }
        let ItemRef::Candidate(index) = entry.item else {
            continue;
        };
        let row = &placed[index].row;
        if first_unidentified_key.is_none()
            && matches!(
                &row.placement,
                TriagePlacement::NeedsYou {
                    reason: crate::import::NeedsYouReason::StillIdentifying { .. },
                    ..
                }
            )
        {
            first_unidentified_key = Some(row.candidate_key.clone());
        }
        if entry.matches_filter && row.selectable {
            if let Some(claim) = row.claim.clone() {
                ready.push(ReadyRowRef {
                    candidate_key: row.candidate_key.clone(),
                    claim,
                    cover_thumbnail_url: row
                        .matched
                        .as_ref()
                        .and_then(|matched| matched.cover_thumbnail_url.clone()),
                });
            }
        }
    }
    ImportQueueSummary {
        counts,
        watched_folders: rows.watched_folders.clone(),
        folder_scan_statuses: rows.folder_scan_statuses.clone(),
        group_keys,
        ready,
        first_unidentified_key,
    }
}

/// The tab's items in order: a header before each run of entries sharing a
/// group, and the entries themselves unless the group is folded shut.
fn emit(view: &ImportListView, ordered: &[OrderedEntry]) -> (Vec<ItemRef>, Vec<GroupHeaderRow>) {
    let entries: Vec<&OrderedEntry> = ordered
        .iter()
        .filter(|entry| entry.tab == view.tab && entry.matches_filter)
        .collect();
    let mut items = Vec::with_capacity(entries.len());
    let mut headers = Vec::new();
    let mut start = 0;
    while start < entries.len() {
        let head = entries[start];
        let mut end = start + 1;
        while end < entries.len()
            && entries[end].watched_folder_path == head.watched_folder_path
            && group_key(entries[end]) == group_key(head)
        {
            end += 1;
        }
        if let Some(group) = head.group.clone() {
            let expanded = !view.collapsed_groups.contains(&group.key);
            headers.push(GroupHeaderRow {
                group,
                watched_folder_path: head.watched_folder_path.clone(),
                expanded,
                entry_count: (end - start) as u32,
            });
            items.push(ItemRef::Header(headers.len() - 1));
            if !expanded {
                start = end;
                continue;
            }
        }
        items.extend(entries[start..end].iter().map(|entry| entry.item));
        start = end;
    }
    (items, headers)
}

fn group_key(entry: &OrderedEntry) -> Option<&FolderReleaseDecisionKey> {
    entry.group.as_ref().map(|group| &group.key)
}

/// The header item one [`GroupHeaderRow`] renders as.
impl GroupHeaderRow {
    pub(crate) fn item(&self) -> ImportListItem {
        ImportListItem::GroupHeader {
            group: self.group.clone(),
            watched_folder_path: self.watched_folder_path.clone(),
            expanded: self.expanded,
            entry_count: self.entry_count,
        }
    }
}
