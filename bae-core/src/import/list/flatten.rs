//! Placement columns in, an ordered list of item references out, in one pass
//! that reads no files or fetched releases.
//!
//! The filter matches only the text each row shows.

use super::pending_filter::pending_filter_entries;
use super::{
    GroupHeaderRow, ImportCandidateListLocation, ImportListItem, ImportListNarrowing,
    ImportListOrder, ImportListRequest, ImportListView, ImportQueueSummary, NarrowedCount,
    PendingFilter, PendingFilterEntry, PlacedRow, UploadStanding, UploadStatus,
};
use crate::db::{ImportQueueRows, ScanCandidateKind, ScanCandidateListRow};
use crate::import::selection::SelectionSummary;
use crate::import::triage::{
    import_status_of, place, CandidateActionBasis, CandidateLiveState, MatchedRelease,
    PendingStanding, StoredLookup, TriageGroup, TriageImportStatus, TriageReading, TriageRow,
    TriageTab, TriageTabCounts,
};
use crate::import::watched_folder::candidate_relative_path;
use crate::import::folder_scanner::ScanItem;
use crate::import::FolderReleaseDecisionKey;
use crate::library::LibraryError;
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

/// Where one item in the list comes from; only the requested windows build
/// theirs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemRef {
    /// Into [`Flattened::headers`].
    Header(usize),
    /// Into [`Flattened::rows`].
    Candidate { index: usize, is_group_member: bool },
    /// Into [`ImportQueueRows::candidates`], for a folder the scan found
    /// invalid.
    Invalid { index: usize, is_group_member: bool },
}

/// The whole queue, flattened for one view.
pub(crate) struct Flattened {
    pub(crate) items: Vec<ItemRef>,
    pub(crate) headers: Vec<GroupHeaderRow>,
    pub(crate) rows: Vec<PlacedRow>,
    pub(crate) summary: ImportQueueSummary,
    /// The key of every row the list shows, in its order.
    pub(crate) shown_keys: Vec<String>,
    /// The request's selected rows the list shows, in its order.
    pub(crate) selected: crate::import::selection::SelectedRows,
    /// Every entry of Found's filter with how many of Found's rows it holds,
    /// whatever the view shows.
    pub(crate) pending_filter_entries: Vec<PendingFilterEntry>,
    /// What the selected rows offer, with what is running for each.
    pub(crate) selection: SelectionSummary,
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
    /// How a Done row sorts; other tabs sort by the folder's date.
    done_order: Option<DoneOrder>,
}

/// A Done row's sort key: outstanding uploads first, then when it was
/// imported.
struct DoneOrder {
    upload_rank: u8,
    imported_at: Option<i64>,
}

/// Every entry of the queue, placed and sorted, before the tab filter and
/// grouping.
struct Ordered {
    entries: Vec<OrderedEntry>,
    placed: Vec<PlacedRow>,
    counts: TriageTabCounts,
}

pub(crate) fn flatten(
    rows: &ImportQueueRows,
    request: &ImportListRequest,
) -> Result<Flattened, LibraryError> {
    let Ordered {
        entries: ordered,
        placed,
        counts,
    } = order(rows, request)?;
    let (items, headers) = emit(&request.view, &ordered);
    // Every row the list shows, and the selected ones among them, in its
    // order: a selected row the list does not show is no row at all to it.
    let mut shown_keys = Vec::new();
    let mut selected = crate::import::selection::SelectedRows::default();
    let mut first_selected_position = None;
    for (position, item) in items.iter().enumerate() {
        let key = match item {
            ItemRef::Candidate { index, .. } => &placed[*index].row.candidate_key,
            ItemRef::Invalid { index, .. } => &rows.candidates[*index].path,
            ItemRef::Header(_) => continue,
        };
        shown_keys.push(key.clone());
        if !request.selected.contains(key) {
            continue;
        }
        first_selected_position.get_or_insert(position as u64);
        match item {
            ItemRef::Candidate { index, .. } => {
                let row = &placed[*index].row;
                selected
                    .candidates
                    .push(crate::import::selection::SelectedCandidate {
                        candidate_key: row.candidate_key.clone(),
                        name: row.folder_name.clone(),
                        basis: row.action_basis.clone(),
                    });
            }
            ItemRef::Invalid { .. } => selected.invalid.push(key.clone()),
            ItemRef::Header(_) => {}
        }
    }
    let summary = summarise(
        rows,
        request,
        &ordered,
        &placed,
        counts,
        first_selected_position,
    );
    let pending_filter_entries = pending_filter_entries(
        placed
            .iter()
            .filter_map(|placed| placed.live.standing.as_ref().map(PendingStanding::state)),
    );
    let selection = SelectionSummary::of(&selected, &rows.standings);
    Ok(Flattened {
        items,
        headers,
        rows: placed,
        summary,
        shown_keys,
        selected,
        pending_filter_entries,
        selection,
    })
}

/// The first candidate in the queue's unfiltered order whose identification
/// is queued, running or having its answer written.
pub(crate) fn first_identifying_candidate(
    rows: &ImportQueueRows,
    request: &ImportListRequest,
) -> Result<Option<String>, LibraryError> {
    let Ordered {
        entries, placed, ..
    } = order(rows, &unfiltered(request))?;
    Ok(entries.iter().find_map(|entry| match entry.item {
        ItemRef::Candidate { index, .. } => {
            let placed = &placed[index];
            placed
                .live
                .facts
                .identifying()
                .then(|| placed.row.candidate_key.clone())
        }
        ItemRef::Header(_) | ItemRef::Invalid { .. } => None,
    }))
}

fn order(rows: &ImportQueueRows, request: &ImportListRequest) -> Result<Ordered, LibraryError> {
    let view = &request.view;
    let filter = TextFilter::of(view);
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
                    // The row shows what its tags call the release, or else
                    // its folder's name.
                    matches_filter: filter.keeps(|| {
                        Ok(match &row.skipped_tags {
                            Some(tags) => std::iter::once(tags.album_title.as_str())
                                .chain(tags.artist.as_deref())
                                .map(Cow::Borrowed)
                                .collect(),
                            None => vec![Cow::Borrowed(row.name.as_str())],
                        })
                    })?,
                    item: ItemRef::Invalid {
                        index,
                        is_group_member: false,
                    },
                    done_order: None,
                });
            }
            // A tentative candidate is not a row until a later scan settles
            // what encloses it.
            ScanCandidateKind::Tentative => {}
            ScanCandidateKind::Valid => {
                let triage_row = place_row(rows, row, &request.selected)?;
                let live = CandidateLiveState::of(
                    &triage_row.action_basis,
                    rows.standing_of(&triage_row.candidate_key),
                );
                let tab = triage_row.placement.tab();
                counts.bump(tab);
                let kept = match tab {
                    TriageTab::Pending => view.pending_filter.holds(
                        live.standing
                            .as_ref()
                            .expect("a row on Found has a standing")
                            .state(),
                    ),
                    // Done is ordered by where uploads stand; until the queue
                    // first answers there is no order to show its rows in.
                    TriageTab::Done => request.upload_status != UploadStatus::Waiting,
                    TriageTab::Skipped => true,
                };
                let matches_filter = kept
                    && filter.keeps(|| {
                        Ok(triage_row.shown_text().into_iter().map(Cow::Borrowed).collect())
                    })?;
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
                        .then(|| done_order(rows, row, &triage_row, &request.upload_status)),
                });
                placed.push(PlacedRow {
                    row: triage_row,
                    live,
                    index,
                });
            }
        }
    }

    let grouped_roots = grouped_roots(rows);
    let combinable_roots = combinable_roots(rows);
    for entry in &mut ordered {
        // Only Pending rows group: how a folder is read is settled on the
        // other tabs.
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
    // A group is dated by its newest member, so date sorting keeps its rows
    // together.
    let mut group_dates: HashMap<FolderReleaseDecisionKey, Option<i64>> = HashMap::new();
    for entry in &ordered {
        if let Some(group) = &entry.group {
            group_dates
                .entry(group.key.clone())
                .and_modify(|date| *date = (*date).max(entry.discovered_at))
                .or_insert(entry.discovered_at);
        }
    }
    // Tab first, so each tab's entries are contiguous.
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

    Ok(Ordered {
        entries: ordered,
        placed,
        counts,
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

pub(super) fn natural_path(left: &str, right: &str) -> std::cmp::Ordering {
    natord::compare_ignore_case(left, right).then_with(|| left.cmp(right))
}

/// The keys of the releases among `items` in path order: by watched folder
/// path, then by each release's path below it — the order the list runs one
/// watched folder's releases in when it orders by path. A folder's reading and
/// its sidecar files are no rows, so they have no key here.
pub(crate) fn release_keys_in_path_order<'a>(
    items: impl IntoIterator<Item = &'a ScanItem>,
) -> Vec<String> {
    let mut releases: Vec<(&str, &str, String)> = items
        .into_iter()
        .filter_map(|item| match item {
            ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => Some((
                candidate.watched_folder_path.as_str(),
                candidate.display_path.as_str(),
                candidate.key(),
            )),
            ScanItem::Invalid(candidate) => Some((
                candidate.watched_folder_path.as_str(),
                candidate.display_path.as_str(),
                candidate.key(),
            )),
            ScanItem::Decided { .. } | ScanItem::Sidecar(_) => None,
        })
        .collect();
    releases.sort_by(|left, right| {
        natural_path(left.0, right.0).then_with(|| natural_path(left.1, right.1))
    });
    releases.into_iter().map(|(_, _, key)| key).collect()
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

/// Where `candidate_key` sits in its tab, unfiltered, with its group opened.
pub(crate) fn locate_candidate(
    rows: &ImportQueueRows,
    request: &ImportListRequest,
    candidate_key: &str,
) -> Result<Option<ImportCandidateListLocation>, LibraryError> {
    let mut request = unfiltered(request);
    let initial = flatten(rows, &request)?;
    let Some(placed) = initial
        .rows
        .iter()
        .find(|placed| placed.row.candidate_key == candidate_key)
    else {
        return Ok(None);
    };
    let tab = placed.row.placement.tab();
    let group = if tab == TriageTab::Pending {
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
    request.view.tab = tab;
    if let Some(group) = &group {
        request.view.collapsed_groups.remove(&group.key);
    }
    let flat = flatten(rows, &request)?;
    let position = flat.items.iter().position(|item| match item {
        ItemRef::Candidate { index, .. } => flat.rows[*index].row.candidate_key == candidate_key,
        ItemRef::Header(_) | ItemRef::Invalid { .. } => false,
    });
    Ok(position.map(|position| ImportCandidateListLocation {
        stable_key: super::candidate_stable_key(candidate_key),
        tab,
        group_key: group.map(|group| group.key),
        visible_position: position as u64,
    }))
}

/// `request` with its filters cleared.
fn unfiltered(request: &ImportListRequest) -> ImportListRequest {
    let mut request = request.clone();
    request.view.filter_text.clear();
    request.view.pending_filter = PendingFilter::All;
    request
}

/// One settled candidate's row, as the tables place it.
pub(super) fn place_row(
    rows: &ImportQueueRows,
    row: &ScanCandidateListRow,
    selected: &BTreeSet<String>,
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
        imported,
        row.error(),
        rows.failures.get(content_hash),
    );
    let skipped = match &row.grouping {
        Some(grouping) => grouping.skipped,
        None => rows.skipped.contains(&(
            row.watched_folder_path.clone(),
            candidate_relative_path(&row.watched_folder_path, Path::new(&row.folder))?,
        )),
    };
    let metadata_provenance = state.and_then(|state| state.metadata_provenance.clone());
    let release_link = state.and_then(|state| state.release_link.clone());
    let draft_valid = state.is_some_and(|state| state.metadata_draft_valid);
    let placement = place(skipped, imported.is_some(), import_status.as_ref());
    let actionable = row.error().is_none();
    let action_basis = CandidateActionBasis::of(
        actionable,
        &placement,
        draft_valid,
        verdict.map(StoredLookup::of),
        row.grouping.is_some(),
        PendingStanding::stored(placement, release_link.as_ref(), verdict),
    );
    let triage_row = TriageRow {
        candidate_key: row.path.clone(),
        folder_name: row.name.clone(),
        watched_folder_path: row.watched_folder_path.clone(),
        display_path: row.display_path.clone(),
        actionable,
        action_basis,
        matched: verdict.and_then(MatchedRelease::of_summary),
        // The window that builds the row reads the linked releases' records
        // and replaces this reading.
        reading: TriageReading::of(
            state.and_then(|state| state.metadata_summary.as_ref()),
            release_link.as_ref(),
            Vec::new(),
        ),
        metadata_summary: state.and_then(|state| state.metadata_summary.clone()),
        cover: None,
        placement,
        import_status,
        metadata_provenance,
        release_link,
        selected: selected.contains(&row.path),
    };
    Ok(triage_row)
}

/// The order tabs' entries run in the sorted list.
fn tab_rank(tab: TriageTab) -> u8 {
    match tab {
        TriageTab::Pending => 0,
        TriageTab::Done => 1,
        TriageTab::Skipped => 2,
    }
}

/// Where one Done row sorts: by its release's upload standing, then import
/// time. With the queue unreadable every row ranks alike, so import time alone
/// orders them.
fn done_order(
    rows: &ImportQueueRows,
    row: &ScanCandidateListRow,
    triage_row: &TriageRow,
    upload_status: &UploadStatus,
) -> DoneOrder {
    let Some(TriageImportStatus::Complete { release }) = &triage_row.import_status else {
        unreachable!("a Done row carries the release imported from it");
    };
    let upload_rank = match upload_status {
        UploadStatus::Read(standing) => {
            UploadStanding::rank(standing.get(&release.release_id).copied())
        }
        UploadStatus::Unreadable | UploadStatus::Waiting => 0,
    };
    DoneOrder {
        upload_rank,
        imported_at: row
            .content_hash
            .as_deref()
            .and_then(|hash| rows.imported_at.get(hash).copied()),
    }
}

/// The first path components with a candidate nested below them; rows under
/// one of those group.
fn grouped_roots(rows: &ImportQueueRows) -> HashSet<(String, String)> {
    let mut grouped = HashSet::new();
    for row in &rows.candidates {
        if matches!(row.kind, ScanCandidateKind::Tentative) {
            continue;
        }
        let mut components = row
            .display_path
            .split('/')
            .filter(|component| !component.is_empty());
        if let (Some(first), Some(_)) = (components.next(), components.next()) {
            grouped.insert((row.watched_folder_path.clone(), first.to_string()));
        }
    }
    grouped
}

/// The folders the list offers to read as one release: those a stored reading
/// keeps apart, and, with nothing stored, the nearest folder above a release
/// that holds two releases or more.
fn combinable_roots(rows: &ImportQueueRows) -> HashSet<(String, String)> {
    let releases: Vec<&ScanCandidateListRow> = rows
        .candidates
        .iter()
        .filter(|row| !matches!(row.kind, ScanCandidateKind::Tentative))
        .collect();
    let mut held: HashMap<(String, String), usize> = HashMap::new();
    for row in &releases {
        for ancestor in ancestors(&row.display_path) {
            *held
                .entry((row.watched_folder_path.clone(), ancestor))
                .or_default() += 1;
        }
    }
    let mut combinable: HashSet<(String, String)> = rows
        .folder_readings
        .iter()
        .filter(|(_, combined)| !**combined)
        .map(|(folder, _)| folder.clone())
        .collect();
    for row in &releases {
        let nearest = ancestors(&row.display_path)
            .into_iter()
            .rev()
            .find(|ancestor| {
                let folder = (row.watched_folder_path.clone(), ancestor.clone());
                !rows.folder_readings.contains_key(&folder)
                    && held.get(&folder).copied().unwrap_or(0) >= 2
            });
        if let Some(nearest) = nearest {
            combinable.insert((row.watched_folder_path.clone(), nearest));
        }
    }
    combinable
}

/// Every folder above the release at `display_path`, shallowest first.
fn ancestors(display_path: &str) -> Vec<String> {
    let components: Vec<&str> = display_path
        .split('/')
        .filter(|component| !component.is_empty())
        .collect();
    (1..components.len())
        .map(|depth| components[..depth].join("/"))
        .collect()
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

/// The view's filter text, lowercased; `None` keeps every row.
struct TextFilter(Option<String>);

impl TextFilter {
    fn of(view: &ImportListView) -> Self {
        Self(view.filters().then(|| view.filter_text.to_lowercase()))
    }

    /// Whether a row showing `shown` survives the filter; `shown` is only
    /// called when there is one.
    fn keeps<'a>(
        &self,
        shown: impl FnOnce() -> Result<Vec<Cow<'a, str>>, LibraryError>,
    ) -> Result<bool, LibraryError> {
        let Some(needle) = &self.0 else {
            return Ok(true);
        };
        Ok(shown()?
            .iter()
            .any(|value| value.to_lowercase().contains(needle.as_str())))
    }
}

/// The whole queue's tab counts, Pending's covers and group keys, and what
/// narrows the tab `view` shows.
fn summarise(
    rows: &ImportQueueRows,
    request: &ImportListRequest,
    ordered: &[OrderedEntry],
    placed: &[PlacedRow],
    counts: TriageTabCounts,
    first_selected_position: Option<u64>,
) -> ImportQueueSummary {
    let view = &request.view;
    let mut group_keys = Vec::new();
    let mut seen_groups = HashSet::new();
    let mut pending_covers = Vec::new();
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
        if entry.matches_filter && entry.tab == TriageTab::Pending {
            pending_covers.extend(
                row.matched
                    .as_ref()
                    .and_then(|matched| matched.cover.clone()),
            );
        }
    }
    let narrowed = view.narrows().then(|| NarrowedCount {
        shown: ordered
            .iter()
            .filter(|entry| entry.tab == view.tab && entry.matches_filter)
            .count() as u32,
        total: counts.of(view.tab),
    });
    ImportQueueSummary {
        narrowed,
        counts,
        watched_folders: rows.watched_folders.clone(),
        group_keys,
        pending_covers,
        narrowing: ImportListNarrowing::of(view),
        first_selected_position,
        done_upload_status: (view.tab == TriageTab::Done).then(|| request.upload_status.known()),
    }
}

/// The tab's items: a header before each group's run, and the entries unless
/// the group is collapsed.
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
