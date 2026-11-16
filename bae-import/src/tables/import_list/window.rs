//! A window of the list, loaded whole in one statement: the range the window
//! reads from the placement's order indexes, and every row of each item in
//! it, joined to that range by the item's key, so the statement reads only
//! the window's rows, however many items the list holds.
//!
//! A release's row comes from its placement — what it shows, its state and
//! what its commands are decided from — and the keyed rows beside it: its
//! scan, its link and the records of the stored releases the link names, its
//! draft's source, its cover and what is running for it. A row with an
//! imported release reads the library release in their place — its cover
//! and its catalog records — and how that release's upload stands. None of
//! it opens the folder's files. A group's header says how many of its releases the
//! filters keep, whether the list folds it, and whether its folder is offered
//! to be read as one release, as its placement holds it.

use super::actions::action_columns;
use super::listed::{ListedColumns, LISTED_COLUMNS};
use super::order::Item;
use super::*;
use crate::candidate_detail::ListedRelease;
use crate::list::{ImportListItem, ImportWindow};
use crate::tables::folder_scans::{
    self, cover_version_joins, CoverVersionColumns, COVER_VERSION_COLUMNS,
};
use crate::tables::import_state::{
    cover_read_columns, read_cover, read_provenance, read_release_link, RuntimeFactColumns,
    StoredCover, StoredProvenance, StoredReleaseLink, PROVENANCE_COLUMNS, RELEASE_LINK_COLUMNS,
    RUNTIME_FACT_COLUMNS,
};
use crate::tables::release_groupings::block_of;
use crate::tables::source_releases::{claimed_records_of_json, claimed_records_sql};
use crate::triage::{
    import_status_of, TriageGroup, TriageMetadataSummary, TriageReading, TriageRow,
};
use crate::RelativeFolder;
use crate::{CandidateKey, FolderReleaseDecisionKey, ImportedRelease, ReleaseLink};
use bae_catalogs::records::ReleaseRecord;
use bae_records::records::queued_uploads::{
    release_upload_progress_in_row, RELEASES_SENDING, RELEASE_UPLOAD_COUNT_COLUMNS,
};
use bae_records::records::read::{
    records_column, release_records_json_sql, release_records_kind_sql,
};
use bae_records::UploadProgress;
use bae_store::ListWindow;
use rusqlite::ToSql;
use std::path::PathBuf;

/// Reads the window `window` of the list `list` in one statement: the items
/// of its range, the header of the group they start inside of, and the keys
/// of the first item and of the item beyond the window's far end.
///
/// The range is the statement's first table. The statement numbers the
/// range's items in the order the parts of its UNION return them; the one
/// past the limit is the item beyond, and a window read backward numbers
/// from its far end, so its items come out in the window's order. A member
/// of a group at the window's start leads with a row for its header, which
/// is not one of the window's items.
pub(super) fn read_window(
    sql: &SqlReadContext<'_>,
    list: &ListRow,
    window: &ListWindow,
) -> Result<ImportWindow, DbError> {
    let items = list.items(Shown::View);
    let Some(range) = items.range_statement(window)? else {
        return Ok(ImportWindow {
            read: bae_store::WindowRead {
                window: window.clone(),
                rows: Vec::new(),
                first: None,
                beyond: None,
            },
            leading_header: None,
        });
    };
    let position = match range.backward {
        true => "min(w.total, :shown) - w.n",
        false => "w.n - 1",
    };
    let statement = format!(
        "WITH keyed AS ( \
             SELECT row_number() OVER () AS n, count(*) OVER () AS total, k.* \
             FROM ({range}) AS k) \
         {rows}",
        range = range.sql(),
        rows = rows_statement(position, &range.columns),
    );
    let shown = bae_store::list_windows::sql_bound(range.limit);
    let wanted = bae_store::list_windows::sql_bound(range.limit.saturating_add(1));
    let range_params = range.params();
    let mut named: Vec<(&str, &dyn ToSql)> =
        range_params.iter().map(|(name, value)| (name.as_str(), *value)).collect();
    named.push((":limit", &wanted));
    named.push((":shown", &shown));
    let rows = items.query(sql, &statement, &named, |row| {
        let role: String = row.get("role")?;
        let item: String = row.get("item")?;
        let ordered = items.led_key(row)?;
        let stored = match (role.as_str(), item.as_str()) {
            ("beyond", _) => None,
            (_, "group") => Some(StoredItem::Header(read_header(row)?)),
            _ => Some(StoredItem::Release(Box::new(read_release(row)?))),
        };
        Ok((role, item, ordered, stored))
    })?;
    let mut read = Vec::new();
    let (mut leading_header, mut first, mut beyond) = (None, None, None);
    for (role, item, ordered, stored) in rows {
        let shown = match stored {
            Some(StoredItem::Header(header)) => Some(header_item(header)?),
            Some(StoredItem::Release(release)) => Some(release_item(*release, Item::of(&item)?)?),
            None => None,
        };
        match (role.as_str(), shown) {
            ("leading", item) => leading_header = item,
            ("beyond", _) => beyond = Some(ordered),
            (_, Some(item)) => {
                if read.is_empty() {
                    first = Some(ordered);
                }
                read.push(item);
            }
            (_, None) => return Err(DbError::unreadable("a row of an import list's window", role)),
        }
    }
    Ok(ImportWindow {
        read: bae_store::WindowRead { window: window.clone(), rows: read, first, beyond },
        leading_header,
    })
}

/// What one row of the window's statement holds, by its kind.
enum StoredItem {
    Header(HeaderRow),
    Release(Box<ReleaseRows>),
}

/// One group header's columns.
struct HeaderRow {
    root: PathBuf,
    folder: RelativeFolder,
    name: String,
    expanded: bool,
    entry_count: i64,
    combinable: bool,
}

/// The header columns of a row of a group whose key is the expression `key`.
/// The entry count counts the placement rows under the group's folder that
/// the pending filter keeps, unless a text filter is active, when it counts
/// the matching rows instead. The placement's `combinable` column says
/// whether bae offers to import the folder as one release
/// (`import_list_recombine`).
fn header_columns(key: &str) -> String {
    format!(
        "hp.group_name AS header_name, \
         NOT EXISTS (SELECT 1 FROM import_list_collapsed AS c \
                     WHERE c.list_id = :list AND c.watched_folder_path = w.watched_folder_path \
                       AND c.group_folder = {key}) AS header_expanded, \
         CASE WHEN hp.key IS NULL THEN NULL \
              WHEN (SELECT pattern IS NOT NULL FROM import_list_view WHERE id = :list) \
              THEN (SELECT count(*) FROM import_list_matching AS m \
                    WHERE m.list_id = :list AND m.watched_folder_path = w.watched_folder_path \
                      AND m.group_folder = {key}) \
              ELSE (SELECT COUNT(*) FROM import_list_view AS v \
                    JOIN import_list_placement AS m \
                        ON m.watched_folder_path = w.watched_folder_path \
                       AND m.group_folder = {key} AND m.item <> 'group' \
                       AND (v.pending_filter = 'all' OR m.pending_entry = v.pending_filter) \
                    WHERE v.id = :list) END AS header_entries, \
         hp.combinable AS header_combinable"
    )
}

fn read_header(row: &Row<'_>) -> rusqlite::Result<HeaderRow> {
    Ok(HeaderRow {
        root: path_column::get(row, "header_root")?,
        folder: row.get("header_folder")?,
        name: row.get("header_name")?,
        expanded: row.get("header_expanded")?,
        entry_count: row.get("header_entries")?,
        combinable: row.get("header_combinable")?,
    })
}

fn header_item(header: HeaderRow) -> Result<ImportListItem, DbError> {
    let HeaderRow { root, folder, name, expanded, entry_count, combinable } = header;
    Ok(ImportListItem::GroupHeader {
        group: TriageGroup {
            key: FolderReleaseDecisionKey {
                watched_folder_path: root.clone(),
                relative_folder: folder,
            },
            name,
            combinable,
        },
        watched_folder_path: root,
        expanded,
        entry_count: to_u32(entry_count, "a group's entry count")?,
    })
}

/// One release's columns, read for the window that holds it.
struct ReleaseRows {
    watched_folder_path: PathBuf,
    key: CandidateKey,
    display_path: String,
    /// What the row shows: an imported release's library album title and
    /// credits, else its draft's title and artists while it shows its draft,
    /// else nothing past its folder's name.
    summary: Option<TriageMetadataSummary>,
    listed: ListedColumns,
    name: String,
    /// Whether the candidate's state describes its current files, which is
    /// when its draft, link, lookup and cover are its own.
    imported: Option<ImportedRelease>,
    /// The version of the imported release's library cover, if it has one.
    library_cover: Option<String>,
    block: (Option<String>, Option<String>, Option<String>),
    failure: Option<crate::ImportFailureReason>,
    facts: RuntimeFactColumns,
    selected: bool,
    /// What the scan read of a folder that failed validation.
    invalid: Option<folder_scans::InvalidColumns>,
    /// The header of the group the release is listed under, by its stable
    /// key.
    header_key: Option<String>,
    /// How the imported release's upload stands.
    upload: Option<UploadProgress>,
    /// The catalogs the imported release's records name.
    library_records: Vec<ReleaseRecord>,
    /// What the imported release's records identify it as, as
    /// [`release_records_kind_sql`] states it.
    library_records_kind: Option<String>,
    /// The records of what the candidate's link names, merged.
    link_records: Option<String>,
    /// The draft's source, the link and the cover, which are the
    /// candidate's own only while its state describes its current files.
    provenance: Option<StoredProvenance>,
    link: Option<StoredReleaseLink>,
    cover: Option<StoredCover>,
    cover_versions: CoverVersionColumns,
}

/// The statement's last part: each item of the range, a group's header or a
/// release's rows, by its key, in the window's order. A group's header reads
/// its placement; a release reads its placement and the keyed rows beside it.
fn rows_statement(position: &str, columns: &str) -> String {
    use crate::tables::import_state::failure_rows::FAILURE_REASON_COLUMNS;
    let invalid_columns = folder_scans::invalid_columns_sql("c");
    let covers = cover_version_joins(
        "(CASE WHEN origin.release_id IS NULL THEN cover.kind END)",
        "cover.file_id",
    );
    let link_records = claimed_records_sql("link.content_hash");
    // The statement's rows are each of the range's items once as a `row`, the
    // first of them again as the `leading` header of its group when it is a
    // member of one, and the item beyond the window as the `beyond` row.
    let item = "CASE r.role WHEN 'leading' THEN 'group' ELSE w.item END";
    let key = "CASE r.role WHEN 'leading' THEN w.group_folder ELSE w.key END";
    let header_columns = header_columns(key);
    format!(
        "SELECT p.watched_folder_path, p.key, p.display_path, \
                p.has_summary, p.shown_title, p.shown_credits, {LISTED_COLUMNS}, \
                c.name, c.content_hash, s.content_hash IS NOT NULL, \
                origin.release_id, released.album_id, \
                g.blocked, g.blocked_subject, g.blocked_holder, \
                failure.content_hash IS NOT NULL AS failed, {FAILURE_REASON_COLUMNS}, \
                {RUNTIME_FACT_COLUMNS}, \
                EXISTS (SELECT 1 FROM import_list_selection AS chosen \
                        WHERE chosen.list_id = :list AND chosen.key = p.key) AS selected, \
                p.group_folder AS group_folder, \
                {library_cover} AS library_cover, \
                {PROVENANCE_COLUMNS}, {RELEASE_LINK_COLUMNS}, {cover}, {invalid}, {actions}, \
                CASE r.role WHEN 'row' THEN {position} WHEN 'leading' THEN -1 \
                     ELSE :shown END AS pos, \
                r.role AS role, {item} AS item, w.part AS part, \
                w.watched_folder_path AS header_root, {key} AS header_folder, {header_columns}, \
                {RELEASE_UPLOAD_COUNT_COLUMNS}, \
                CASE WHEN origin.release_id IS NOT NULL THEN {library_records} END \
                    AS library_records, \
                {library_records_kind} AS library_records_kind, \
                CASE WHEN link.content_hash IS NOT NULL AND origin.release_id IS NULL \
                     THEN ({link_records}) END AS link_records, \
                {COVER_VERSION_COLUMNS}, {columns} \
         FROM keyed AS w \
         JOIN (SELECT 'row' AS role UNION ALL SELECT 'leading' UNION ALL SELECT 'beyond') AS r \
             ON CASE r.role WHEN 'row' THEN w.n <= :shown \
                            WHEN 'beyond' THEN w.n > :shown \
                            ELSE w.n <= :shown AND ({position}) = 0 AND w.item <> 'group' \
                                 AND w.group_folder IS NOT NULL END \
         LEFT JOIN import_list_placement AS hp \
             ON r.role <> 'beyond' AND {item} = 'group' \
            AND hp.watched_folder_path = w.watched_folder_path \
            AND hp.item = 'group' AND hp.key = {key} \
         LEFT JOIN import_list_placement AS p \
             ON r.role <> 'beyond' AND {item} <> 'group' \
            AND p.watched_folder_path = w.watched_folder_path \
            AND p.key = {key} AND p.item <> 'group' \
         LEFT JOIN scan_candidate AS c \
             ON c.watched_folder_path = p.watched_folder_path AND c.path = p.key \
         LEFT JOIN release_grouping AS g ON g.key = c.grouping_key \
         LEFT JOIN import_candidate_state AS s \
             ON s.content_hash = c.content_hash AND s.edit_revision = c.file_edit_revision \
         LEFT JOIN local_release_origin AS origin ON origin.content_hash = c.content_hash \
         LEFT JOIN releases AS released ON released.id = origin.release_id \
         LEFT JOIN import_candidate_failure AS failure \
             ON failure.content_hash = c.content_hash \
         LEFT JOIN import_candidate_standing AS st ON st.candidate_key = p.key \
         LEFT JOIN import_candidate_draft_provenance AS provenance \
             ON provenance.content_hash = s.content_hash \
         LEFT JOIN import_candidate_release_link AS link ON link.content_hash = s.content_hash \
         LEFT JOIN import_candidate_cover AS cover ON cover.content_hash = s.content_hash \
         LEFT JOIN release_storage AS storage \
             ON storage.release_id = origin.release_id AND storage.queued_count > 0 \
         LEFT JOIN ({RELEASES_SENDING}) AS sending ON sending.release_id = storage.release_id \
         {covers} \
         {invalid_join} \
         ORDER BY pos",
        cover = cover_read_columns(),
        library_cover = bae_records::records::files::cover_version_sql("origin.release_id"),
        library_records = release_records_json_sql("origin.release_id"),
        library_records_kind = release_records_kind_sql("origin.release_id"),
        invalid = invalid_columns.0,
        invalid_join = invalid_columns.1,
        actions = action_columns(),
    )
}

/// One release's columns of a row of [`rows_statement`].
fn read_release(row: &Row<'_>) -> rusqlite::Result<ReleaseRows> {
    use crate::tables::import_state::failure_rows::failure_reason_from_row;
    let summary = match row.get::<_, Option<bool>>(3)? {
        Some(true) => Some(TriageMetadataSummary {
            album_title: row.get(4)?,
            album_artists: crate::tables::stored_credits::line_column(row, "shown_credits")?
                .unwrap_or_default(),
        }),
        Some(false) | None => None,
    };
    let failed: bool = row.get("failed")?;
    Ok(ReleaseRows {
        watched_folder_path: path_column::get(row, 0)?,
        key: row.get(1)?,
        display_path: row.get(2)?,
        summary,
        listed: ListedColumns::read(row, 6)?,
        name: row.get(16)?,
        imported: match row.get::<_, Option<String>>(19)? {
            Some(release_id) => Some(ImportedRelease { release_id, album_id: row.get(20)? }),
            None => None,
        },
        library_cover: row.get("library_cover")?,
        block: (row.get(21)?, row.get(22)?, row.get(23)?),
        failure: match failed {
            true => Some(failure_reason_from_row(row)?),
            false => None,
        },
        facts: RuntimeFactColumns::read(row, 26)?,
        selected: row.get("selected")?,
        invalid: folder_scans::InvalidColumns::read(row)?,
        header_key: row
            .get::<_, Option<RelativeFolder>>("group_folder")?
            .map(|folder| {
                Ok::<_, rusqlite::Error>(crate::list::group_stable_key(&FolderReleaseDecisionKey {
                    watched_folder_path: path_column::get(row, 0)?,
                    relative_folder: folder,
                }))
            })
            .transpose()?,
        upload: release_upload_progress_in_row(row)?,
        library_records: match row.get::<_, Option<String>>("library_records")? {
            Some(_) => records_column(row, "library_records")?,
            None => Vec::new(),
        },
        library_records_kind: row.get("library_records_kind")?,
        link_records: row.get("link_records")?,
        provenance: read_provenance(row)?,
        link: read_release_link(row)?,
        cover: read_cover(row)?,
        cover_versions: CoverVersionColumns::read(row)?,
    })
}

/// A release's item: a folder that failed validation as its scan read it, or
/// a candidate as its placement and the keyed rows beside it say.
/// How the row of a release this device imported reads: as what the library
/// release's records identify it as, with those records.
fn library_reading(
    kind: Option<&str>,
    records: Vec<ReleaseRecord>,
) -> Result<TriageReading, DbError> {
    match kind {
        None => Ok(TriageReading::Prefilled),
        Some("pressing") => Ok(TriageReading::Identified { records }),
        Some("album") => Ok(TriageReading::IdentifiedAlbum { records }),
        Some(other) => Err(DbError::unreadable("what a library release's records identify", other)),
    }
}

fn release_item(release: ReleaseRows, item: Item) -> Result<ImportListItem, DbError> {
    match item {
        Item::Invalid => invalid_item(release),
        Item::Candidate => candidate_item(release),
        Item::Group => Err(DbError::unreadable("the placement of a release's row", "group")),
    }
}

/// A folder that failed validation, as its row shows it: what its scan read,
/// which the row's own statement selected.
fn invalid_item(release: ReleaseRows) -> Result<ImportListItem, DbError> {
    let Some(invalid) = release.invalid else {
        return Err(DbError::unreadable(
            format!("the scan item of {}, listed as failing validation", release.key),
            release.name,
        ));
    };
    let candidate = invalid.candidate(&release.key, release.name, release.watched_folder_path)?;
    Ok(ImportListItem::Invalid { candidate, selected: release.selected })
}

/// A candidate's row: its placement and the keyed rows beside it, listed
/// under its group's header, if it has one.
fn candidate_item(release: ReleaseRows) -> Result<ImportListItem, DbError> {
    let ReleaseRows {
        watched_folder_path,
        key,
        display_path,
        summary,
        listed,
        name,
        imported,
        library_cover,
        block,
        failure,
        facts,
        selected,
        header_key,
        upload,
        library_records,
        library_records_kind,
        link_records,
        provenance,
        link,
        cover,
        cover_versions,
        ..
    } = release;
    let ListedRelease { basis, actions, standing } = listed.listed(&key)?;
    let facts = facts.facts(&key)?;
    let (blocked, subject, holder) = block;
    let block = block_of(blocked, subject, holder)?;
    let import_status = import_status_of(imported.as_ref(), block.as_ref(), failure.as_ref());
    let shown = match imported.as_ref() {
        // A release this device imported reads the library release: its
        // cover, and the catalogs its records name.
        Some(imported) => ShownRows {
            metadata_provenance: None,
            release_link: None,
            reading: library_reading(library_records_kind.as_deref(), library_records)?,
            cover: bae_records::cover_ref(&imported.release_id, library_cover)
                .map(|image| crate::CoverImageSource::Library { image }),
        },
        None => {
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
                reading: TriageReading::of(summary.as_ref(), release_link.as_ref(), records),
                release_link,
                cover,
            }
        }
    };
    let row = TriageRow {
        candidate_key: key,
        folder_name: name,
        watched_folder_path,
        display_path,
        actionable: basis.actionable,
        placement: basis.placement,
        action_basis: basis,
        reading: shown.reading,
        metadata_summary: summary,
        cover: shown.cover,
        import_status,
        upload: imported.and(upload),
        metadata_provenance: shown.metadata_provenance,
        release_link: shown.release_link,
        selected,
    };
    Ok(ImportListItem::Candidate {
        row,
        live: crate::CandidateLiveState::of(actions, facts, standing),
        header_key,
    })
}

/// What a release's row shows past its placement: the candidate's draft
/// source, link and cover, or for an imported release the library
/// release's.
struct ShownRows {
    metadata_provenance: Option<crate::MetadataProvenance>,
    release_link: Option<ReleaseLink>,
    reading: TriageReading,
    cover: Option<crate::CoverImageSource>,
}

#[cfg(test)]
#[path = "window_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "row_kinds_tests.rs"]
mod row_kinds_tests;
