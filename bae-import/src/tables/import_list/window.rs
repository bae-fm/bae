//! A window of the list, loaded whole in one statement: the compiled read of
//! the list's order (`window_reads`), which joins the placement's two sides
//! from the window's first key on and reads what each item's row shows beside
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

use super::order::{Item, Items};
use super::row_columns::{self, PlacementColumns, ReleaseColumns, ShownColumns};
use super::window_reads::*;
use super::*;
use crate::candidate_detail::ListedRelease;
use crate::list::{ImportGroupMember, ImportListItem, ImportWindow};
use crate::tables::import_state::{claimed_records_of_json, StoredCover, StoredReleaseLink};
use crate::tables::release_groupings::block_of;
use crate::triage::{
    import_status_of, TriageListLiveState, TriageReading, TriageRow, TriageRowStatus,
};
use crate::{CandidateKey, ImportListOrder, ReleaseLink};
use bae_catalogs::records::ReleaseRecord;
use bae_records::UploadProgress;
use bae_store::list_windows::{OrderKey, SortValue, WindowRead};
use bae_store::ListWindow;
use bae_types::credits::CreditLine;
#[cfg(test)]
use rusqlite::ToSql;
#[cfg(test)]
use sqlive_compose::Statement as _;

/// The columns of a window, before they are shaped into the rows the list shows.
pub(super) type WindowRows = WindowRead<StoredItem>;

pub(super) fn process_window(rows: WindowRows) -> Result<ImportWindow, DbError> {
    rows.try_map_rows(StoredItem::shaped)
}

/// One item's columns of a row of a window's read.
#[derive(Clone, PartialEq)]
pub(super) struct StoredItem {
    item: String,
    release: Option<StoredRelease>,
    /// The `ListedHeader` document.
    header: String,
    ends_group: Option<bool>,
}

/// The columns of a release's item.
#[derive(Clone, PartialEq)]
struct StoredRelease {
    watched_folder_path: Vec<u8>,
    key: CandidateKey,
    has_summary: Option<bool>,
    shown_title: Option<String>,
    shown_credits: Option<String>,
    tab: String,
    import_failed: Option<bool>,
    actionable: Option<bool>,
    pending_state: Option<String>,
    needs_you_reason: Option<String>,
    reason_pressings: Option<i64>,
    error_detail: Option<String>,
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
    let rows = WindowStatement::of(request, window)?.read(sql)?;
    Ok(WindowRows { window: window.clone(), rows })
}

/// The columns of one row of a window's read; each order's row type holds
/// them under the same names.
macro_rules! stored_item {
    ($row:ident) => {{
        let header =
            $row.header.expect("a header fragment gives a document for every row").listed_header;
        let release = match ($row.release_key, $row.release_root) {
            (Some(key), Some(root)) => Some(StoredRelease {
                watched_folder_path: root,
                key,
                has_summary: $row.has_summary,
                shown_title: $row.shown_title,
                shown_credits: $row.shown_credits,
                tab: required($row.tab, "tab")?,
                import_failed: $row.import_failed,
                actionable: $row.actionable,
                pending_state: $row.pending_state,
                needs_you_reason: $row.needs_you_reason,
                reason_pressings: $row.reason_pressings,
                error_detail: $row.error_detail,
                actions: required($row.actions, "actions")?.actions,
                row: required($row.listed, "listed")?.listed_row,
            }),
            _ => None,
        };
        Ok::<_, DbError>(StoredItem {
            item: $row.item,
            release,
            header,
            ends_group: $row.ends_group,
        })
    }};
}

/// The window's statement: the read of the list's order, with what it binds.
pub(super) enum WindowStatement {
    NewestFirst(NewestFirstWindowParams),
    OldestFirst(OldestFirstWindowParams),
    PathAscending(PathAscendingWindowParams),
    PathDescending(PathDescendingWindowParams),
}

impl WindowStatement {
    /// The statement reading the window `window` of the list `request` shows.
    /// A window from the list's top binds a key whose first value sorts
    /// before every row's, which the read's comparisons take every row past.
    pub(super) fn of(request: &ImportListRequest, window: &ListWindow) -> Result<Self, DbError> {
        let items = Items::new(request.clone());
        let mut key = items.window_key(window.from.as_ref())?.into_iter();
        let mut next = || key.next().expect("a window's key has a value for each of its terms");
        let tab = tab_column(request.view.tab).to_string();
        let filter = request.entry().map(|filter| pending_filter_column(filter).to_string());
        let filter_text = request.view.search.as_ref().map(|text| text.as_str().to_string());
        let folds_only = matches!(request.folds, crate::list::ImportListFolds::Only(_));
        let folded = request.folded_json();
        let limit = bae_store::list_windows::sql_bound(window.limit);
        Ok(match request.view.order {
            ImportListOrder::NewestFirst => Self::NewestFirst(NewestFirstWindowParams {
                tab,
                filter,
                filter_text,
                folds_only,
                folded,
                limit,
                order_key_0: next(),
                order_key_1: next(),
                order_key_2: next(),
                order_key_3: next(),
                order_key_4: next(),
                order_key_5: next(),
                order_key_6: next(),
                order_key_7: next(),
                order_key_8: next(),
                order_key_9: next(),
                order_key_10: next(),
                order_key_11: next(),
                order_key_12: next(),
                order_key_13: next(),
            }),
            ImportListOrder::OldestFirst => Self::OldestFirst(OldestFirstWindowParams {
                tab,
                filter,
                filter_text,
                folds_only,
                folded,
                limit,
                order_key_0: next(),
                order_key_1: next(),
                order_key_2: next(),
                order_key_3: next(),
                order_key_4: next(),
                order_key_5: next(),
                order_key_6: next(),
                order_key_7: next(),
                order_key_8: next(),
                order_key_9: next(),
                order_key_10: next(),
                order_key_11: next(),
                order_key_12: next(),
                order_key_13: next(),
            }),
            ImportListOrder::PathAscending => Self::PathAscending(PathAscendingWindowParams {
                tab,
                filter,
                filter_text,
                folds_only,
                folded,
                limit,
                order_key_0: next(),
                order_key_1: next(),
                order_key_2: next(),
                order_key_3: next(),
                order_key_4: next(),
                order_key_5: next(),
                order_key_6: next(),
                order_key_7: next(),
                order_key_8: next(),
                order_key_9: next(),
            }),
            ImportListOrder::PathDescending => Self::PathDescending(PathDescendingWindowParams {
                tab,
                filter,
                filter_text,
                folds_only,
                folded,
                limit,
                order_key_0: next(),
                order_key_1: next(),
                order_key_2: next(),
                order_key_3: next(),
                order_key_4: next(),
                order_key_5: next(),
                order_key_6: next(),
                order_key_7: next(),
                order_key_8: next(),
                order_key_9: next(),
            }),
        })
    }

    /// The statement's SQL and the values it binds.
    #[cfg(test)]
    pub(super) fn parts(&self) -> (&'static str, Vec<(&'static str, &dyn ToSql)>) {
        match self {
            Self::NewestFirst(params) => (NewestFirstWindow::SQL, params.params()),
            Self::OldestFirst(params) => (OldestFirstWindow::SQL, params.params()),
            Self::PathAscending(params) => (PathAscendingWindow::SQL, params.params()),
            Self::PathDescending(params) => (PathDescendingWindow::SQL, params.params()),
        }
    }

    /// The items the statement reads, each with its key.
    fn read(&self, sql: &impl coven::SqlRead) -> Result<Vec<(StoredItem, OrderKey)>, DbError> {
        Ok(match self {
            Self::NewestFirst(params) => bae_store::statement_rows(sql, params)?
                .into_iter()
                .map(|row| {
                    let key = OrderKey::new(vec![
                        row.order_key_0.into_key_value(),
                        row.order_key_1.into_key_value(),
                        row.order_key_2.into_key_value(),
                        row.order_key_3.into_key_value(),
                        row.order_key_4.into_key_value(),
                        row.order_key_5.into_key_value(),
                        row.order_key_6.into_key_value(),
                        row.order_key_7.into_key_value(),
                        row.order_key_8.into_key_value(),
                        row.order_key_9.into_key_value(),
                        row.order_key_10.into_key_value(),
                        row.order_key_11.into_key_value(),
                        row.order_key_12.into_key_value(),
                        row.order_key_13.into_key_value(),
                    ]);
                    let item = stored_item!(row);
                    Ok((item?, key))
                })
                .collect::<Result<_, DbError>>()?,
            Self::OldestFirst(params) => bae_store::statement_rows(sql, params)?
                .into_iter()
                .map(|row| {
                    let key = OrderKey::new(vec![
                        row.order_key_0.into_key_value(),
                        row.order_key_1.into_key_value(),
                        row.order_key_2.into_key_value(),
                        row.order_key_3.into_key_value(),
                        row.order_key_4.into_key_value(),
                        row.order_key_5.into_key_value(),
                        row.order_key_6.into_key_value(),
                        row.order_key_7.into_key_value(),
                        row.order_key_8.into_key_value(),
                        row.order_key_9.into_key_value(),
                        row.order_key_10.into_key_value(),
                        row.order_key_11.into_key_value(),
                        row.order_key_12.into_key_value(),
                        row.order_key_13.into_key_value(),
                    ]);
                    let item = stored_item!(row);
                    Ok((item?, key))
                })
                .collect::<Result<_, DbError>>()?,
            Self::PathAscending(params) => bae_store::statement_rows(sql, params)?
                .into_iter()
                .map(|row| {
                    let key = OrderKey::new(vec![
                        row.order_key_0.into_key_value(),
                        row.order_key_1.into_key_value(),
                        row.order_key_2.into_key_value(),
                        row.order_key_3.into_key_value(),
                        row.order_key_4.into_key_value(),
                        row.order_key_5.into_key_value(),
                        row.order_key_6.into_key_value(),
                        row.order_key_7.into_key_value(),
                        row.order_key_8.into_key_value(),
                        row.order_key_9.into_key_value(),
                    ]);
                    let item = stored_item!(row);
                    Ok((item?, key))
                })
                .collect::<Result<_, DbError>>()?,
            Self::PathDescending(params) => bae_store::statement_rows(sql, params)?
                .into_iter()
                .map(|row| {
                    let key = OrderKey::new(vec![
                        row.order_key_0.into_key_value(),
                        row.order_key_1.into_key_value(),
                        row.order_key_2.into_key_value(),
                        row.order_key_3.into_key_value(),
                        row.order_key_4.into_key_value(),
                        row.order_key_5.into_key_value(),
                        row.order_key_6.into_key_value(),
                        row.order_key_7.into_key_value(),
                        row.order_key_8.into_key_value(),
                        row.order_key_9.into_key_value(),
                    ]);
                    let item = stored_item!(row);
                    Ok((item?, key))
                })
                .collect::<Result<_, DbError>>()?,
        })
    }
}

/// The plans SQLite reads the windows of the list `request` shows by, one
/// line per step: from the list's top, then from its first item. What a test
/// holds each order to its indexes with.
#[cfg(test)]
pub(super) fn window_plans(
    sql: &impl coven::SqlRead,
    request: &ImportListRequest,
) -> Result<Vec<Vec<String>>, DbError> {
    let top = bae_store::ListWindow::top(50);
    let first = WindowStatement::of(request, &top)?.read(sql)?.into_iter().next();
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

/// A key column of a window's row as the value a key holds.
trait IntoKeyValue {
    fn into_key_value(self) -> SortValue;
}

impl IntoKeyValue for i64 {
    fn into_key_value(self) -> SortValue {
        SortValue::Integer(self)
    }
}

impl IntoKeyValue for String {
    fn into_key_value(self) -> SortValue {
        SortValue::Text(self)
    }
}

impl IntoKeyValue for Vec<u8> {
    fn into_key_value(self) -> SortValue {
        SortValue::Blob(self)
    }
}

impl<T: IntoKeyValue> IntoKeyValue for Option<T> {
    fn into_key_value(self) -> SortValue {
        self.map_or(SortValue::Null, IntoKeyValue::into_key_value)
    }
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
        let header = row_columns::header(&self.header)?;
        let item = Item::of(&self.item)?;
        if item == Item::Group {
            let header = required(header, "header")?;
            return Ok(ImportListItem::GroupHeader(header));
        }
        let release = required(self.release, "placement")?;
        let group = header
            .map(|header| {
                Ok::<_, DbError>(ImportGroupMember {
                    header,
                    last: required(self.ends_group, "ends_group")?,
                })
            })
            .transpose()?;
        let columns = row_columns::release_columns(&release.row)?;
        match item {
            Item::Candidate => candidate_item(release, columns, group),
            Item::Invalid | Item::Group => invalid_item(release, columns),
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
        tab,
        import_failed,
        actionable,
        pending_state,
        needs_you_reason,
        reason_pressings,
        error_detail,
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
    let placement = PlacementColumns {
        tab,
        import_failed,
        actionable,
        pending_state,
        needs_you_reason,
        reason_pressings,
        error_detail,
    };
    let listed = placement.listed(&actions)?;
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
        ShownColumns::Candidate { link_records, provenance, link, cover, cover_versions } => {
            let release_link = link.map(StoredReleaseLink::link).transpose()?;
            let records = match link_records {
                Some(json) => claimed_records_of_json(&json)?,
                None => Vec::new(),
            };
            let cover = cover
                .map(StoredCover::selection)
                .transpose()?
                .map(|selection| {
                    cover_versions.choice(&key, &selection).map(crate::CoverChoice::into_image)
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
