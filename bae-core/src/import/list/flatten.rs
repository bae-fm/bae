//! Placement columns in, an ordered list of item references out.
//!
//! One pass over the queue answers every question the tab asks: where each
//! entry sits, which group it joins, which tab it counts against, whether the
//! filter keeps it, and — for the chrome — the Ready set, the group keys and
//! the row the identify count is still waiting on. Nothing here reads a file,
//! a cue sheet, a boundary tree or an archived document; those are loaded for
//! the items inside the requested windows and nowhere else.

use super::{
    ActiveFolderScan, FirstUnidentifiedRowRef, FolderScanActivity, GroupHeaderRow,
    ImportCandidateListLocation, ImportListItem, ImportListOrder, ImportListRequest,
    ImportListView, ImportQueueSummary, PlacedRow, ReadyRowRef, UploadStanding,
};
use crate::db::{ImportQueueRows, ScanCandidateKind, ScanCandidateListRow};
use crate::identify::classify_summary;
use crate::import::folder_registry::candidate_relative_path;
use crate::import::triage::{
    import_status_of, place, CandidateAnswer, MatchedRelease, TriageGroup, TriageImportStatus,
    TriagePlacement, TriageRow, TriageRuntimeFacts, TriageTab, TriageTabCounts,
};
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
    Candidate { index: usize, is_group_member: bool },
    /// Into [`ImportQueueRows::candidates`] — a row the scan found invalid.
    Invalid { index: usize, is_group_member: bool },
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
    discovered_at: Option<i64>,
    tab: TriageTab,
    group: Option<TriageGroup>,
    matches_filter: bool,
    item: ItemRef,
    /// How a Done row sorts against its neighbours. `None` on every other tab,
    /// which uses the source folder's date instead.
    done_order: Option<DoneOrder>,
}

/// Outstanding uploads remain first. Date sorting on Done describes when
/// the release entered the library rather than when its folder was discovered.
struct DoneOrder {
    upload_rank: u8,
    imported_at: Option<i64>,
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
                    discovered_at: row.discovered_at,
                    tab: TriageTab::Skipped,
                    group: None,
                    matches_filter: matches_text(
                        view,
                        [row.name.as_str(), row.display_path.as_str()],
                    ),
                    item: ItemRef::Invalid {
                        index,
                        is_group_member: false,
                    },
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
                    discovered_at: row.discovered_at,
                    tab,
                    group: None,
                    matches_filter,
                    item: ItemRef::Candidate {
                        index: placed.len(),
                        is_group_member: false,
                    },
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
        if let ItemRef::Candidate { index, .. } = entry.item {
            if !matches!(
                rows.candidates[placed[index].index].source,
                crate::db::CandidateListSource::Folder
            ) {
                continue;
            }
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
    // A group's most recent member dates the group as a whole. Compare the
    // group before its members, so dates never scatter a group's rows between
    // other headers. Compute this before filtering/collapsing: neither changes
    // when the source folder was last added to.
    let mut group_dates: HashMap<FolderReleaseDecisionKey, Option<i64>> = HashMap::new();
    for entry in &ordered {
        if let Some(group) = &entry.group {
            group_dates
                .entry(group.key.clone())
                .and_modify(|date| *date = (*date).max(entry.discovered_at))
                .or_insert(entry.discovered_at);
        }
    }
    // Sort every tab's entries in one pass, tab first so each tab's run is
    // contiguous and its own order is decided among its own rows. Which tab is
    // being shown is the filter's business, further down.
    ordered.sort_by(|left, right| {
        tab_rank(left.tab).cmp(&tab_rank(right.tab)).then_with(|| {
            let upload_order = match (&left.done_order, &right.done_order) {
                (Some(left), Some(right)) => left.upload_rank.cmp(&right.upload_rank),
                _ => std::cmp::Ordering::Equal,
            };
            let (left_path, left_date) = left.sort_group(&group_dates);
            let (right_path, right_date) = right.sort_group(&group_dates);
            let by_root = root_order
                .get(left.watched_folder_path.as_str())
                .cmp(&root_order.get(right.watched_folder_path.as_str()))
                .then_with(|| left.watched_folder_path.cmp(&right.watched_folder_path));
            let by_unit = by_root.then_with(|| natural_path(left_path, right_path));
            let by_member = natural_path(&left.display_path, &right.display_path);
            let chosen = match view.order {
                ImportListOrder::PathAscending => by_unit.then(by_member),
                ImportListOrder::PathDescending => by_unit.then(by_member).reverse(),
                ImportListOrder::NewestFirst | ImportListOrder::OldestFirst => {
                    compare_dates(left_date, right_date, view.order)
                        .then(by_unit)
                        .then_with(|| {
                            compare_dates(left.sort_date(), right.sort_date(), view.order)
                        })
                        .then(by_member)
                }
            };
            upload_order.then(chosen)
        })
    });

    let mut summary = summarise(rows, &ordered, &placed, counts);
    let (items, headers) = emit(view, &ordered);
    if let Some(target) = &mut summary.first_unidentified {
        target.visible_position = items
            .iter()
            .position(|item| match item {
                ItemRef::Candidate { index, .. } => {
                    placed[*index].row.candidate_key == target.candidate_key
                }
                ItemRef::Header(_) | ItemRef::Invalid { .. } => false,
            })
            .map(|position| position as u64);
    }
    Ok(Flattened {
        items,
        headers,
        rows: placed,
        summary,
    })
}

impl OrderedEntry {
    fn sort_group(
        &self,
        group_dates: &HashMap<FolderReleaseDecisionKey, Option<i64>>,
    ) -> (&str, Option<i64>) {
        match &self.group {
            Some(group) => (&group.key.relative_folder_path, group_dates[&group.key]),
            None => (&self.display_path, self.sort_date()),
        }
    }

    fn sort_date(&self) -> Option<i64> {
        match &self.done_order {
            Some(done) => done.imported_at,
            None => self.discovered_at,
        }
    }
}

fn natural_path(left: &str, right: &str) -> std::cmp::Ordering {
    natord::compare_ignore_case(left, right).then_with(|| left.cmp(right))
}

/// Undated, not-yet-rescanned candidates come last in both date directions.
fn compare_dates(
    left: Option<i64>,
    right: Option<i64>,
    order: ImportListOrder,
) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => match order {
            ImportListOrder::NewestFirst => right.cmp(&left),
            ImportListOrder::OldestFirst => left.cmp(&right),
            ImportListOrder::PathAscending | ImportListOrder::PathDescending => {
                unreachable!("date comparison requires a date order")
            }
        },
        _ => right.is_some().cmp(&left.is_some()),
    }
}

/// Locate `candidate_key` using the same placement, grouping, filtering and
/// ordering pass as the list itself. Only the target's group is opened; the
/// caller's disclosure state for every other group remains authoritative.
pub(crate) fn locate_candidate(
    rows: &ImportQueueRows,
    request: &ImportListRequest,
    candidate_key: &str,
) -> Result<Option<ImportCandidateListLocation>, LibraryError> {
    let initial = flatten(rows, request)?;
    let Some(placed) = initial
        .rows
        .iter()
        .find(|placed| placed.row.candidate_key == candidate_key)
    else {
        return Ok(None);
    };
    let tab = placed.row.placement.tab();
    let group = if tab == TriageTab::Pending
        && matches!(
            rows.candidates[placed.index].source,
            crate::db::CandidateListSource::Folder
        ) {
        let source = &rows.candidates[placed.index];
        group_for(
            &source.watched_folder_path,
            &source.display_path,
            &grouped_roots(rows),
            &combinable_roots(rows),
        )
    } else {
        None
    };
    let mut request = request.clone();
    request.view.tab = tab;
    request.view.filter_text.clear();
    if let Some(group) = &group {
        request.view.collapsed_groups.remove(&group.key);
    }
    let flat = flatten(rows, &request)?;
    let position = flat.items.iter().position(|item| match item {
        ItemRef::Candidate { index, .. } => flat.rows[*index].row.candidate_key == candidate_key,
        ItemRef::Header(_) | ItemRef::Invalid { .. } => false,
    });
    Ok(position.map(|position| ImportCandidateListLocation {
        stable_key: ImportListItem::candidate_stable_key(candidate_key),
        tab,
        group_key: group.map(|group| group.key),
        visible_position: position as u64,
    }))
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
        row.source
            .error()
            .or_else(|| rows.failures.get(content_hash).map(String::as_str)),
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
    let known = match (answer, facts.identification.clone()) {
        (Some(classification), _) => CandidateAnswer::Classified(classification),
        (None, Some(status)) => CandidateAnswer::Identification(status),
        (None, None) => CandidateAnswer::Unidentified,
    };
    let skipped = match &row.source {
        crate::db::CandidateListSource::Combination { skipped, .. } => *skipped,
        crate::db::CandidateListSource::Folder => rows.skipped.contains(&(
            row.watched_folder_path.clone(),
            candidate_relative_path(&row.watched_folder_path, Path::new(&row.path))
                .map_err(|error| LibraryError::Internal(error.to_string()))?,
        )),
    };
    let metadata_provenance = state.and_then(|state| state.metadata_provenance.clone());
    let placement = place(
        skipped,
        imported.is_some(),
        import_status.as_ref(),
        metadata_provenance.as_ref(),
        state.is_some_and(|state| state.metadata_draft_valid),
        &known,
    );
    let actions = crate::import::triage::candidate_actions(
        row.source.error().is_none(),
        &placement,
        facts.identification.as_ref(),
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
        actionable: row.source.error().is_none(),
        skip_action: row
            .source
            .error()
            .is_none()
            .then(|| placement.skip_action())
            .flatten(),
        selectable: actions.contains(&crate::import::triage::CandidateAction::ImportReady),
        actions,
        matched: verdict.and_then(MatchedRelease::of_summary),
        metadata_summary: state.and_then(|state| state.metadata_summary.clone()),
        cover_thumbnail: None,
        placement,
        import_status,
        metadata_provenance,
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
        imported_at: row
            .content_hash
            .as_deref()
            .and_then(|hash| rows.imported_at.get(hash).copied()),
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
        if matches!(row.kind, ScanCandidateKind::Tentative)
            || !matches!(row.source, crate::db::CandidateListSource::Folder)
        {
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
    let mut first_unidentified = None;
    for entry in ordered {
        if let Some(group) = &entry.group {
            if seen_groups.insert(group.key.clone()) {
                group_keys.push(group.key.clone());
            }
        }
        let ItemRef::Candidate { index, .. } = entry.item else {
            continue;
        };
        let row = &placed[index].row;
        if first_unidentified.is_none()
            && matches!(&row.placement, TriagePlacement::Identification { .. })
        {
            first_unidentified = Some(FirstUnidentifiedRowRef {
                candidate_key: row.candidate_key.clone(),
                stable_key: ImportListItem::candidate_stable_key(&row.candidate_key),
                group_key: entry.group.as_ref().map(|group| group.key.clone()),
                visible_position: None,
            });
        }
        if entry.matches_filter && row.selectable {
            ready.push(ReadyRowRef {
                candidate_key: row.candidate_key.clone(),
                cover_thumbnail_url: row
                    .matched
                    .as_ref()
                    .and_then(|matched| matched.cover_thumbnail_url.clone()),
            });
        }
    }
    let active_scans: Vec<ActiveFolderScan> = rows
        .folder_scan_statuses
        .iter()
        .filter_map(|folder| match folder.status {
            crate::import::FolderScanStatus::Scanning { found_count } => Some(ActiveFolderScan {
                watched_folder_path: folder.watched_folder_path.clone(),
                watched_folder_name: folder.watched_folder_name.clone(),
                found_count,
            }),
            crate::import::FolderScanStatus::Complete
            | crate::import::FolderScanStatus::Failed { .. } => None,
        })
        .collect();
    let folder_scan_activity = (!active_scans.is_empty()).then(|| FolderScanActivity {
        found_count: active_scans.iter().map(|folder| folder.found_count).sum(),
        folders: active_scans,
    });
    ImportQueueSummary {
        counts,
        watched_folders: rows.watched_folders.clone(),
        folder_scan_statuses: rows.folder_scan_statuses.clone(),
        folder_scan_activity,
        group_keys,
        ready,
        first_unidentified,
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
        let is_group_member = head.group.is_some();
        items.extend(
            entries[start..end]
                .iter()
                .map(|entry| entry.item.with_group_membership(is_group_member)),
        );
        start = end;
    }
    (items, headers)
}

impl ItemRef {
    fn with_group_membership(self, is_group_member: bool) -> Self {
        match self {
            Self::Header(index) => Self::Header(index),
            Self::Candidate { index, .. } => Self::Candidate {
                index,
                is_group_member,
            },
            Self::Invalid { index, .. } => Self::Invalid {
                index,
                is_group_member,
            },
        }
    }
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
