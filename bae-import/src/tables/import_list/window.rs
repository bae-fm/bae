//! A window of the list, loaded whole in one statement: the compiled read of
//! the list's order, which joins the placement's two sides from the window's
//! first key on (`window_reads`), or from the order's first row for a window
//! from the list's top (`top_reads`), and reads what each item's row shows beside
//! its placement through the placement's row fragments (`row_fragments`), so
//! the statement reads only the window's rows, however many items the list
//! holds.
//!
//! A release's row comes from its placement — what it shows and its state on
//! Found — and what the fragments read: the commands it offers, its scan, why
//! a grouping holds it, its link and the records of the stored releases the
//! link names, its draft's source, its cover and what is running for it. A row
//! with an imported release reads the library release in their place — its
//! cover and its catalog records — and how that release's upload stands. None
//! of it opens the folder's files. A group's header says how many of its
//! releases the filters keep, whether the list folds it, and whether its
//! folder is offered to be read as one release, as its placement holds it.
//! Each member of a group reads its group's header beside its own row, and
//! whether the list shows no member of the group after it.

use super::list_items::{key_of, key_values, keyed_params, Item, ListBinding};
use super::listed::{ListedColumns, PlacementColumns};
use super::row_columns::{self, ReleaseColumns, ShownColumns};
use super::top_reads::*;
use super::window_reads::*;
use super::*;
use crate::candidate_detail::ListedRelease;
use crate::list::{ImportGroupMember, ImportListItem, ImportWindow};
use crate::tables::import_state::{claimed_records_of_json, StoredReleaseLink};
use crate::tables::release_groupings::block_of;
use crate::triage::{
    import_status_of, TriageListLiveState, TriageReading, TriageRow, TriageRowStatus,
};
use crate::{CandidateKey, ImportListOrder, ReleaseLink};
use bae_catalogs::records::ReleaseRecord;
use bae_records::UploadProgress;
use bae_store::list_windows::{OrderKey, WindowRead};
use bae_store::ListWindow;
use bae_types::credits::CreditLine;
#[cfg(test)]
use coven::Statement as _;
#[cfg(test)]
use rusqlite::ToSql;

/// The columns of a window, before they are shaped into the rows the list shows.
pub(super) type WindowRows = WindowRead<StoredItem>;

pub(super) fn process_window(rows: WindowRows) -> Result<ImportWindow, DbError> {
    rows.try_map_rows(StoredItem::shaped)
}

/// One item's columns of a row of a window's read, by what the item is.
#[derive(Clone, PartialEq)]
pub(super) enum StoredItem {
    /// A group's header, as its `ListedHeader` document.
    Header(String),
    Candidate {
        release: StoredRelease,
        /// For a member of a group.
        member: Option<StoredMember>,
    },
    /// A folder that failed validation.
    Invalid(StoredRelease),
}

/// What a member of a group reads beside its own row.
#[derive(Clone, PartialEq)]
pub(super) struct StoredMember {
    /// Its group's `ListedHeader` document.
    header: String,
    /// Whether the list shows no member of the group after it.
    last: bool,
}

/// The columns of a release's item.
#[derive(Clone, PartialEq)]
pub(super) struct StoredRelease {
    watched_folder_path: Vec<u8>,
    key: CandidateKey,
    has_summary: Option<bool>,
    shown_title: Option<String>,
    shown_credits: Option<String>,
    placement: PlacementColumns,
    /// The `ListedActions` document.
    actions: String,
    /// The `ListedRow` document.
    row: String,
}

/// Reads the window `window` of the list `request` shows in one statement: the
/// items of its range, each with its key, and each member of a group with its
/// group's header.
pub(super) fn read_window(
    sql: &SqlReadContext<'_>,
    request: &ImportListRequest,
    window: &ListWindow,
) -> Result<WindowRows, DbError> {
    window_of(window, sql.reads(&window_read(request, window)?)?)
}

/// The read of a window's rows.
pub(super) type WindowItems = coven::ReadAs<WindowItemRow>;

/// One row of a window's read, as its order's statement returned it.
#[derive(Clone, PartialEq)]
pub(super) enum WindowItemRow {
    NewestFirstTop(NewestFirstTopRow),
    OldestFirstTop(OldestFirstTopRow),
    PathAscendingTop(PathAscendingTopRow),
    PathDescendingTop(PathDescendingTopRow),
    NewestFirst(NewestFirstWindowRow),
    OldestFirst(OldestFirstWindowRow),
    PathAscending(PathAscendingWindowRow),
    PathDescending(PathDescendingWindowRow),
}

/// The read of the window `window` of the list `request` shows.
pub(super) fn window_read(
    request: &ImportListRequest,
    window: &ListWindow,
) -> Result<WindowItems, DbError> {
    Ok(WindowStatement::of(request, window)?.reads())
}

/// The window `window` as its read returned it.
pub(super) fn window_of(
    window: &ListWindow,
    items: Vec<WindowItemRow>,
) -> Result<WindowRows, DbError> {
    let rows = items.into_iter().map(WindowItemRow::item).collect::<Result<_, _>>()?;
    Ok(WindowRows { window: window.clone(), rows })
}

/// The columns of one row of a window's read; each order's row type holds
/// them under the same names.
macro_rules! stored_item {
    ($row:ident) => {{
        let header =
            $row.header.expect("a header fragment gives a document for every row").listed_header;
        let release = || {
            Ok::<_, DbError>(StoredRelease {
                watched_folder_path: required($row.release_root, "watched folder")?,
                key: required($row.release_key, "key")?,
                has_summary: $row.has_summary,
                shown_title: $row.shown_title,
                shown_credits: $row.shown_credits,
                placement: PlacementColumns {
                    tab: required($row.tab, "tab")?,
                    import_failed: $row.import_failed,
                    actionable: $row.actionable,
                    pending_state: $row.pending_state,
                    needs_you_reason: $row.needs_you_reason,
                    reason_pressings: $row.reason_pressings,
                    error_detail: $row.error_detail,
                },
                actions: required($row.actions, "actions")?.actions,
                row: required($row.listed, "listed")?.listed_row,
            })
        };
        Ok::<_, DbError>(match Item::of(&$row.item)? {
            Item::Group => StoredItem::Header(header),
            Item::Candidate => StoredItem::Candidate {
                release: release()?,
                member: $row.ends_group.map(|last| StoredMember { header, last }),
            },
            Item::Invalid => StoredItem::Invalid(release()?),
        })
    }};
}

/// The window's statement: the read of the list's order from its top, or from
/// a key, with what it binds.
pub(super) enum WindowStatement {
    NewestFirstTop(NewestFirstTopParams),
    OldestFirstTop(OldestFirstTopParams),
    PathAscendingTop(PathAscendingTopParams),
    PathDescendingTop(PathDescendingTopParams),
    NewestFirst(NewestFirstWindowParams),
    OldestFirst(OldestFirstWindowParams),
    PathAscending(PathAscendingWindowParams),
    PathDescending(PathDescendingWindowParams),
}

impl WindowStatement {
    /// The statement reading the window `window` of the list `request` shows:
    /// from the order's first row for a window from the list's top, else from
    /// the window's key.
    pub(super) fn of(request: &ImportListRequest, window: &ListWindow) -> Result<Self, DbError> {
        let ListBinding { tab, filter, filter_text, folds_only, folded } = ListBinding::of(request);
        let limit = bae_store::list_windows::sql_bound(window.limit);
        let Some(from) = &window.from else {
            macro_rules! top {
                ($params:ident) => {
                    $params { tab, filter, filter_text, folds_only, folded, limit }
                };
            }
            return Ok(match request.view.order {
                ImportListOrder::NewestFirst => Self::NewestFirstTop(top!(NewestFirstTopParams)),
                ImportListOrder::OldestFirst => Self::OldestFirstTop(top!(OldestFirstTopParams)),
                ImportListOrder::PathAscending => {
                    Self::PathAscendingTop(top!(PathAscendingTopParams))
                }
                ImportListOrder::PathDescending => {
                    Self::PathDescendingTop(top!(PathDescendingTopParams))
                }
            });
        };
        macro_rules! bound {
            ($params:ident, [$($column:ident)*]) => {
                keyed_params!(
                    $params {
                        tab: tab,
                        filter: filter,
                        filter_text: filter_text,
                        folds_only: folds_only,
                        folded: folded,
                        limit: limit,
                    },
                    key_values(from)?,
                    [$($column)*]
                )
            };
        }
        Ok(match request.view.order {
            ImportListOrder::NewestFirst => Self::NewestFirst(bound!(
                NewestFirstWindowParams,
                [
                    order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
                order_key_10 order_key_11 order_key_12 order_key_13
                ]
            )),
            ImportListOrder::OldestFirst => Self::OldestFirst(bound!(
                OldestFirstWindowParams,
                [
                    order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
                order_key_10 order_key_11 order_key_12 order_key_13
                ]
            )),
            ImportListOrder::PathAscending => Self::PathAscending(bound!(
                PathAscendingWindowParams,
                [
                    order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
                ]
            )),
            ImportListOrder::PathDescending => Self::PathDescending(bound!(
                PathDescendingWindowParams,
                [
                    order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
                ]
            )),
        })
    }

    /// The statement's SQL and the values it binds.
    #[cfg(test)]
    pub(super) fn parts(&self) -> (&'static str, Vec<(&'static str, &dyn ToSql)>) {
        match self {
            Self::NewestFirstTop(params) => (NewestFirstTop::SQL, params.params()),
            Self::OldestFirstTop(params) => (OldestFirstTop::SQL, params.params()),
            Self::PathAscendingTop(params) => (PathAscendingTop::SQL, params.params()),
            Self::PathDescendingTop(params) => (PathDescendingTop::SQL, params.params()),
            Self::NewestFirst(params) => (NewestFirstWindow::SQL, params.params()),
            Self::OldestFirst(params) => (OldestFirstWindow::SQL, params.params()),
            Self::PathAscending(params) => (PathAscendingWindow::SQL, params.params()),
            Self::PathDescending(params) => (PathDescendingWindow::SQL, params.params()),
        }
    }

    /// The read of the rows the statement reads.
    fn reads(self) -> WindowItems {
        match self {
            Self::NewestFirstTop(params) => {
                coven::ReadAs::new(params, WindowItemRow::NewestFirstTop)
            }
            Self::OldestFirstTop(params) => {
                coven::ReadAs::new(params, WindowItemRow::OldestFirstTop)
            }
            Self::PathAscendingTop(params) => {
                coven::ReadAs::new(params, WindowItemRow::PathAscendingTop)
            }
            Self::PathDescendingTop(params) => {
                coven::ReadAs::new(params, WindowItemRow::PathDescendingTop)
            }
            Self::NewestFirst(params) => coven::ReadAs::new(params, WindowItemRow::NewestFirst),
            Self::OldestFirst(params) => coven::ReadAs::new(params, WindowItemRow::OldestFirst),
            Self::PathAscending(params) => coven::ReadAs::new(params, WindowItemRow::PathAscending),
            Self::PathDescending(params) => {
                coven::ReadAs::new(params, WindowItemRow::PathDescending)
            }
        }
    }
}

impl WindowItemRow {
    /// The item the row shows, with its key.
    fn item(self) -> Result<(StoredItem, OrderKey), DbError> {
        macro_rules! item {
            ($row:ident, [$($column:ident)*]) => {{
                let key = key_of!($row, $($column)*);
                Ok((stored_item!($row)?, key))
            }};
        }
        match self {
            Self::NewestFirstTop(row) => item!(row, [
                order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
                order_key_10 order_key_11 order_key_12 order_key_13
            ]),
            Self::OldestFirstTop(row) => item!(row, [
                order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
                order_key_10 order_key_11 order_key_12 order_key_13
            ]),
            Self::PathAscendingTop(row) => item!(row, [
                order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
            ]),
            Self::PathDescendingTop(row) => item!(row, [
                order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
            ]),
            Self::NewestFirst(row) => item!(row, [
                order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
                order_key_10 order_key_11 order_key_12 order_key_13
            ]),
            Self::OldestFirst(row) => item!(row, [
                order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
                order_key_10 order_key_11 order_key_12 order_key_13
            ]),
            Self::PathAscending(row) => item!(row, [
                order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
            ]),
            Self::PathDescending(row) => item!(row, [
                order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
            ]),
        }
    }
}

/// The plans SQLite reads the windows of the list `request` shows by, one
/// line per step: from the list's top, then from its first item. What a test
/// holds each order to its indexes with.
#[cfg(test)]
pub(super) fn window_plans(
    sql: &SqlReadContext<'_>,
    request: &ImportListRequest,
) -> Result<Vec<Vec<String>>, DbError> {
    let top = bae_store::ListWindow::top(50);
    let first = read_window(sql, request, &top)?.rows.into_iter().next();
    let mut windows = vec![top];
    if let Some((_, key)) = first {
        windows.push(ListWindow { from: Some(key), limit: 50 });
    }
    windows
        .iter()
        .map(|window| {
            let statement = WindowStatement::of(request, window)?;
            let (statement, params) = statement.parts();
            Ok(sql.query(&format!("EXPLAIN QUERY PLAN {statement}"), params.as_slice(), |row| {
                row.get::<_, String>(3)
            })?)
        })
        .collect()
}

/// A column every release's row holds.
fn required<T>(column: Option<T>, name: &str) -> Result<T, DbError> {
    column.ok_or_else(|| {
        DbError::unreadable(format!("a release's {name} in a window"), None::<String>)
    })
}

impl StoredItem {
    /// The item as the list shows it.
    fn shaped(self) -> Result<ImportListItem, DbError> {
        let header = |json: &str| required(row_columns::header(json)?, "header");
        match self {
            Self::Header(json) => Ok(ImportListItem::GroupHeader(header(&json)?)),
            Self::Candidate { release, member } => {
                let group = member
                    .map(|member| {
                        Ok::<_, DbError>(ImportGroupMember {
                            header: header(&member.header)?,
                            last: member.last,
                        })
                    })
                    .transpose()?;
                let columns = row_columns::release_columns(&release.row)?;
                candidate_item(release, columns, group)
            }
            Self::Invalid(release) => {
                let columns = row_columns::release_columns(&release.row)?;
                invalid_item(release, columns)
            }
        }
    }
}

/// Chooses what the row of a release this device imported shows from what
/// the library release's records identify: a pressing or an album with those
/// records, or a prefilled row when they identify neither. Each shows
/// `album`, the library album's title and credits.
fn library_reading(
    album: crate::AlbumLine,
    kind: Option<&str>,
    records: Vec<ReleaseRecord>,
) -> Result<TriageReading, DbError> {
    match kind {
        None => Ok(TriageReading::Prefilled { album }),
        Some("pressing") => Ok(TriageReading::Identified { album, records }),
        Some("album") => Ok(TriageReading::IdentifiedAlbum { album, records }),
        Some(other) => Err(DbError::unreadable("what a library release's records identify", other)),
    }
}

/// A folder that failed validation, as its row shows it: what its scan read.
fn invalid_item(
    release: StoredRelease,
    columns: ReleaseColumns,
) -> Result<ImportListItem, DbError> {
    let Some(invalid) = columns.invalid else {
        return Err(DbError::unreadable(
            format!("the scan item of {}, listed as failing validation", release.key),
            columns.name,
        ));
    };
    let root = path_column::decode(release.watched_folder_path)?;
    let row = invalid.row(release.key, columns.name, root)?;
    Ok(ImportListItem::Invalid { row })
}

/// A candidate's row: its placement and what the row fragments read beside
/// it, listed under its group's header, if it has one.
fn candidate_item(
    release: StoredRelease,
    columns: ReleaseColumns,
    group: Option<ImportGroupMember>,
) -> Result<ImportListItem, DbError> {
    let StoredRelease {
        watched_folder_path,
        key,
        has_summary,
        shown_title,
        shown_credits,
        placement,
        actions,
        row: _,
    } = release;
    let watched_folder_path = path_column::decode(watched_folder_path)?;
    // What the row shows: an imported release's library album title and
    // credits, else its draft's title and artists while it shows its draft,
    // else nothing past its folder's name.
    let summary: Option<(String, CreditLine)> = match has_summary {
        Some(true) => Some((
            required(shown_title, "shown_title")?,
            crate::tables::stored_credits::line_text(&required(shown_credits, "shown_credits")?)
                .map_err(|error| {
                    DbError::unreadable("a release's shown credits", error.to_string())
                })?,
        )),
        Some(false) | None => None,
    };
    let listed = ListedColumns::new(placement, super::actions::offered(&actions)?);
    let ReleaseColumns { name, block, failure, facts, shown, .. } = columns;
    let ListedRelease { basis, actions, standing } = listed.listed(&key)?;
    let facts = facts.facts(&key)?;
    let (blocked, subject, holder) = block;
    let block = block_of(blocked, subject, holder)?;
    let import_status = import_status_of(shown.imported(), block.as_ref(), failure.as_ref());
    let album = summary.map(|(title, credits)| crate::AlbumLine::of(title, credits));
    let shown = match shown {
        // A release this device imported reads the library release: its
        // cover, the catalogs its records name, and its upload.
        ShownColumns::Imported { release, cover, records, records_kind, upload } => ShownRows {
            metadata_provenance: None,
            release_link: None,
            reading: library_reading(
                album.ok_or_else(|| {
                    DbError::unreadable(
                        format!("the album of {key}, which this device imported"),
                        None::<String>,
                    )
                })?,
                records_kind.as_deref(),
                records,
            )?,
            cover: bae_records::cover_ref(&release.release_id, cover)
                .map(|image| crate::CoverImageSource::Library { image }),
            upload,
        },
        ShownColumns::Candidate { link_records, provenance, link, cover } => {
            let release_link = link.map(StoredReleaseLink::link).transpose()?;
            let records = match link_records {
                Some(json) => claimed_records_of_json(&json)?,
                None => Vec::new(),
            };
            let cover = cover
                .map(|(stored, versions)| {
                    versions.choice(&key, &stored.selection()?).map(crate::CoverChoice::into_image)
                })
                .transpose()?;
            ShownRows {
                metadata_provenance: provenance.map(|stored| stored.provenance()).transpose()?,
                reading: TriageReading::of(album, release_link.as_ref(), records),
                release_link,
                cover,
                upload: None,
            }
        }
    };
    let status = TriageRowStatus::of(
        import_status.as_ref(),
        shown.upload.as_ref(),
        facts.import,
        standing.as_ref(),
    );
    let live =
        TriageListLiveState { identification: facts.identification, actions, standing, status };
    let row = TriageRow {
        candidate_key: key,
        folder_name: name,
        watched_folder_path,
        actionable: basis.actionable,
        placement: basis.placement,
        reading: shown.reading,
        cover: shown.cover,
        import_status,
        metadata_provenance: shown.metadata_provenance,
        release_link: shown.release_link,
    };
    Ok(ImportListItem::Candidate { row, live, group })
}

/// What a release's row shows past its placement: the candidate's draft
/// source, link and cover, or for an imported release the library
/// release's, and its upload.
struct ShownRows {
    metadata_provenance: Option<crate::MetadataProvenance>,
    release_link: Option<ReleaseLink>,
    reading: TriageReading,
    cover: Option<crate::CoverImageSource>,
    upload: Option<UploadProgress>,
}

#[cfg(test)]
#[path = "window_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "row_kinds_tests.rs"]
mod row_kinds_tests;

#[cfg(test)]
#[path = "header_counts_tests.rs"]
mod header_counts_tests;
