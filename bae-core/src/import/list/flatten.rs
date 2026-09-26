//! Placement columns in, an ordered list of item references out.
//!
//! One pass over the queue answers every question the tab asks: where each
//! entry sits, which group it joins, which tab it counts against, whether the
//! filter keeps it, and — for the chrome — the Ready set and the group keys.
//! Nothing here reads a file,
//! a cue sheet, a boundary tree or a fetched release; those are loaded for
//! the items inside the requested windows and nowhere else.
//!
//! The filter tests the text each row shows and nothing else: a candidate
//! row's draft title and artists, or its folder's name when it has no draft;
//! a Done row's library release's title, artists and year; an invalid folder's
//! name. What a row does not show — its path, the verdict's lead — finds
//! nothing.

use super::{
    GroupHeaderRow, ImportCandidateListLocation, ImportListItem, ImportListOrder,
    ImportListRequest, ImportListView, ImportQueueSummary, PlacedRow, ReadyRowRef, UploadStanding,
};
use crate::db::{ImportQueueRows, ScanCandidateKind, ScanCandidateListRow};
use crate::identify::classify_summary;
use crate::import::triage::{
    import_status_of, place, CandidateActionBasis, MatchedRelease, TriageGroup,
    TriageImportStatus, TriageReading, TriageRow, TriageTab, TriageTabCounts,
};
use crate::import::watched_folder::candidate_relative_path;
use crate::import::FolderReleaseDecisionKey;
use crate::library::LibraryError;
use std::borrow::Cow;
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

/// Every entry of the queue, placed and in the order the view sorts them,
/// before the tab filter and the grouping into items runs.
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
    let summary = summarise(rows, &ordered, &placed, counts);
    let (items, headers) = emit(&request.view, &ordered);
    Ok(Flattened {
        items,
        headers,
        rows: placed,
        summary,
    })
}

/// The first of `keys` in the queue's own order — every tab in turn, each in
/// the order `request`'s view sorts it — or `None` when the queue holds none
/// of them. The filter hides none of them, so it is not applied at all.
pub(crate) fn first_candidate_among(
    rows: &ImportQueueRows,
    request: &ImportListRequest,
    keys: &HashSet<String>,
) -> Result<Option<String>, LibraryError> {
    let Ordered {
        entries, placed, ..
    } = order(rows, &unfiltered(request))?;
    Ok(entries.iter().find_map(|entry| match entry.item {
        ItemRef::Candidate { index, .. } => {
            let key = &placed[index].row.candidate_key;
            keys.contains(key).then(|| key.clone())
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
                    matches_filter: filter.keeps(|| Ok(vec![Cow::Borrowed(row.name.as_str())]))?,
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
                let triage_row = place_row(rows, row)?;
                let tab = triage_row.placement.tab();
                counts.bump(tab);
                let matches_filter = filter.keeps(|| shown_text(rows, &triage_row))?;
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

/// Locate `candidate_key` using the same placement, grouping and ordering
/// pass as the list itself. The filter is cleared, so the candidate is where
/// it sits in its tab unfiltered. Only the target's group is opened; the
/// caller's disclosure state for every other group remains authoritative.
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
        stable_key: ImportListItem::candidate_stable_key(candidate_key),
        tab,
        group_key: group.map(|group| group.key),
        visible_position: position as u64,
    }))
}

/// `request` with its filter cleared: the request a queue read without the
/// filter's text answers.
fn unfiltered(request: &ImportListRequest) -> ImportListRequest {
    let mut request = request.clone();
    request.view.filter_text.clear();
    request
}

/// One settled candidate's row, as the tables place it. `matched` is the
/// verdict's lead — the window fills it in for the items it materialises.
pub(super) fn place_row(
    rows: &ImportQueueRows,
    row: &ScanCandidateListRow,
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
        rows.failures.get(content_hash).map(String::as_str),
    );
    let answer = verdict.map(classify_summary);
    let skipped = match &row.grouping {
        Some(grouping) => grouping.skipped,
        None => rows.skipped.contains(&(
            row.watched_folder_path.clone(),
            candidate_relative_path(&row.watched_folder_path, Path::new(&row.folder))
                .map_err(|error| LibraryError::Internal(error.to_string()))?,
        )),
    };
    let metadata_provenance = state.and_then(|state| state.metadata_provenance.clone());
    let placement = place(
        skipped,
        imported.is_some(),
        import_status.as_ref(),
        state.map_or(crate::import::MetadataAuthor::Nobody, |state| {
            state.metadata_author
        }),
        state.is_some_and(|state| state.metadata_draft_valid),
        answer.as_ref(),
    );
    // Every row the list holds is a settled release: a tentative candidate
    // never becomes one.
    let actionable = row.error().is_none();
    let action_basis = CandidateActionBasis::of(actionable, &placement, answer.as_ref());
    Ok(TriageRow {
        candidate_key: row.path.clone(),
        folder_name: row.name.clone(),
        watched_folder_path: row.watched_folder_path.clone(),
        display_path: row.display_path.clone(),
        separable: row.grouping.is_some(),
        actionable,
        selectable: action_basis.importable_at_rest(),
        action_basis,
        matched: verdict.and_then(MatchedRelease::of_summary),
        // The records are read off the pick's stored releases, which the
        // queue never opens: the window that materialises the row reads them
        // and builds the reading over again.
        reading: TriageReading::of(
            state.and_then(|state| state.metadata_summary.as_ref()),
            metadata_provenance.as_ref(),
            Vec::new(),
        ),
        metadata_summary: state.and_then(|state| state.metadata_summary.clone()),
        cover: None,
        ready_check: crate::import::triage::ready_check(&placement),
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
        if matches!(row.kind, ScanCandidateKind::Tentative) {
            continue;
        }
        note(&row.watched_folder_path, &row.display_path, false);
    }
    grouped
}

/// The folders the list can offer to read as one release, counted over the
/// releases as the groupings read them — a folder holding one album read from
/// two disc folders holds one release, and offers nothing.
///
/// Two kinds of folder offer it: one whose releases are kept apart by a
/// stored reading, and one nothing is stored for that is the nearest such
/// folder above some release to hold two releases or more. A folder between
/// that one and the release, holding only the one reading's worth, is where
/// the choice already belongs, so nothing above it is asked.
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
        let nearest = ancestors(&row.display_path).into_iter().rev().find(|ancestor| {
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

/// Every folder above the release at `display_path`, shallowest first, as
/// root-relative paths — the release's own folder left out.
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

/// The view's filter text, lowercased once. `None` for an empty filter, which
/// keeps every row.
struct TextFilter(Option<String>);

impl TextFilter {
    fn of(view: &ImportListView) -> Self {
        Self(
            view.filters()
                .then(|| view.filter_text.to_lowercase()),
        )
    }

    /// Whether a row showing `shown` survives the filter. The text is only
    /// asked for when there is a filter: with none, the queue read did not
    /// read a Done row's.
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

/// Every piece of text one settled candidate's row shows. A Done row is the
/// library release it became, so its text is that release's, read with the
/// queue; every other row is the candidate's own [`TriageRow::shown_text`].
fn shown_text<'a>(
    rows: &'a ImportQueueRows,
    triage_row: &'a TriageRow,
) -> Result<Vec<Cow<'a, str>>, LibraryError> {
    if triage_row.placement != crate::import::TriagePlacement::Done {
        return Ok(triage_row
            .shown_text()
            .into_iter()
            .map(Cow::Borrowed)
            .collect());
    }
    let Some(TriageImportStatus::Complete { release }) = &triage_row.import_status else {
        return Err(LibraryError::Internal(format!(
            "candidate {} is placed Done with no imported release",
            triage_row.candidate_key
        )));
    };
    let texts = rows.imported_text.as_ref().ok_or_else(|| {
        LibraryError::Internal(
            "the import queue was read without its Done rows' text, which the filter tests"
                .to_string(),
        )
    })?;
    let text = texts.get(&release.release_id).ok_or_else(|| {
        LibraryError::Internal(format!(
            "candidate {} is placed Done on release {}, which has no album",
            triage_row.candidate_key, release.release_id
        ))
    })?;
    Ok(text.shown_text())
}

/// The chrome, over the whole queue rather than the requested tab: the counts
/// every tab bar shows, the Ready set the foot bar acts on, and every group key
/// disclosure state is retained against.
fn summarise(
    rows: &ImportQueueRows,
    ordered: &[OrderedEntry],
    placed: &[PlacedRow],
    counts: TriageTabCounts,
) -> ImportQueueSummary {
    let mut group_keys = Vec::new();
    let mut seen_groups = HashSet::new();
    let mut ready = Vec::new();
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
        if entry.matches_filter && row.selectable {
            ready.push(ReadyRowRef {
                candidate_key: row.candidate_key.clone(),
                cover: row
                    .matched
                    .as_ref()
                    .and_then(|matched| matched.cover.clone()),
            });
        }
    }
    ImportQueueSummary {
        counts,
        watched_folders: rows.watched_folders.clone(),
        group_keys,
        ready,
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
