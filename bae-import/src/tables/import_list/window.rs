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
//! to be read as one release, as its placement holds it. Each member of a
//! group reads its group's header beside its own row, and whether the list
//! shows no member of the group after it.

use super::actions::action_columns;
use super::listed::{ListedColumns, LISTED_COLUMNS};
use super::order::Item;
use super::*;
use crate::candidate_detail::ListedRelease;
use crate::list::{ImportGroupHeader, ImportGroupMember, ImportListItem, ImportWindow};
use crate::tables::folder_scans::{
    self, cover_version_joins, CoverVersionColumns, COVER_VERSION_COLUMNS,
};
use crate::tables::import_state::{claimed_records_of_json, claimed_records_sql};
use crate::tables::import_state::{
    cover_read_columns, read_cover, read_provenance, read_release_link, RuntimeFactColumns,
    StoredCover, StoredProvenance, StoredReleaseLink, PROVENANCE_COLUMNS, RELEASE_LINK_COLUMNS,
    RUNTIME_FACT_COLUMNS,
};
use crate::tables::release_groupings::block_of;
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
/// of its range, each with its key, and each member of a group with its
/// group's header.
///
/// The range is the statement's first table. The statement numbers the
/// range's items in the order the parts of its UNION return them, and reads
/// each item's header beside it. The statement names the range once: each
/// further reference to it would expand the range's views again, and a
/// range over a text filter's views would pass SQLite's limit on the
/// cursors one statement opens.
pub(super) fn read_window(
    sql: &SqlReadContext<'_>,
    list: &ListRow,
    window: &ListWindow,
) -> Result<ImportWindow, DbError> {
    let items = list.items();
    let Some(range) = items.range_statement(window)? else {
        return Ok(ImportWindow { window: window.clone(), rows: Vec::new() });
    };
    let statement = format!(
        "WITH keyed AS ( \
             SELECT row_number() OVER () AS n, k.* \
             FROM ({range}) AS k) \
         {rows}",
        range = range.sql(),
        rows = rows_statement(&range.columns, &items.ends_group("w")),
    );
    let limit = bae_store::list_windows::sql_bound(range.limit);
    let range_params = range.params();
    let mut named: Vec<(&str, &dyn ToSql)> =
        range_params.iter().map(|(name, value)| (name.as_str(), *value)).collect();
    named.push((":limit", &limit));
    let rows = items.query(sql, &statement, &named, |row| {
        let item: String = row.get("item")?;
        let ordered = items.led_key(row)?;
        let stored = match item.as_str() {
            "group" => StoredItem::Header(read_header(row)?),
            _ => StoredItem::Release(Box::new(read_release(row)?)),
        };
        Ok((item, ordered, stored))
    })?;
    let mut read = Vec::new();
    for (item, ordered, stored) in rows {
        let shown = match stored {
            StoredItem::Header(header) => ImportListItem::GroupHeader(group_header(header)?),
            StoredItem::Release(release) => release_item(*release, Item::of(&item)?)?,
        };
        read.push((shown, ordered));
    }
    Ok(ImportWindow { window: window.clone(), rows: read })
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

/// The key of the group whose header the row of item `w` shows: the item's
/// own for a header, its group's for a member, and none for a release in no
/// group.
const HEADER_FOLDER: &str = "CASE w.item WHEN 'group' THEN w.key ELSE w.group_folder END";

/// The header columns of the row of item `w`: its own for a header, its
/// group's for a member, from the header's placement `hp`, and none for a
/// release in no group. The entry count counts the placement rows under the
/// group's folder that the pending filter keeps, unless a text filter is
/// active, when it counts the matching rows instead. The placement's
/// `combinable` column says whether bae offers to import the folder as one
/// release (`import_list_recombine`).
fn header_columns() -> String {
    format!(
        "w.watched_folder_path AS header_root, {HEADER_FOLDER} AS header_folder, \
         hp.group_name AS header_name, \
         NOT EXISTS (SELECT 1 FROM import_list_collapsed AS c \
                     WHERE c.list_id = :list AND c.watched_folder_path = w.watched_folder_path \
                       AND c.group_folder = {HEADER_FOLDER}) AS header_expanded, \
         CASE WHEN hp.key IS NULL THEN NULL \
              WHEN (SELECT pattern IS NOT NULL FROM import_list_view WHERE id = :list) \
              THEN (SELECT count(*) FROM import_list_matching AS m \
                    WHERE m.list_id = :list AND m.watched_folder_path = w.watched_folder_path \
                      AND m.group_folder = {HEADER_FOLDER}) \
              ELSE (SELECT COUNT(*) FROM import_list_view AS v \
                    JOIN import_list_placement AS m \
                        ON m.watched_folder_path = w.watched_folder_path \
                       AND m.group_folder = {HEADER_FOLDER} AND m.item <> 'group' \
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

/// A group member's columns: its group's header, and whether the list shows
/// no member of the group after it.
struct MemberRow {
    header: HeaderRow,
    last: bool,
}

/// The group of the row of a release, when it is a member of one.
fn read_member(row: &Row<'_>) -> rusqlite::Result<Option<MemberRow>> {
    match row.get::<_, Option<RelativeFolder>>("header_folder")? {
        Some(_) => Ok(Some(MemberRow { header: read_header(row)?, last: row.get("ends_group")? })),
        None => Ok(None),
    }
}

fn group_header(header: HeaderRow) -> Result<ImportGroupHeader, DbError> {
    let HeaderRow { root, folder, name, expanded, entry_count, combinable } = header;
    Ok(ImportGroupHeader {
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
    /// What the row shows: an imported release's library album title and
    /// credits, else its draft's title and artists while it shows its draft,
    /// else nothing past its folder's name.
    summary: Option<TriageMetadataSummary>,
    listed: ListedColumns,
    name: String,
    block: (Option<String>, Option<String>, Option<String>),
    failure: Option<crate::ImportFailureReason>,
    facts: RuntimeFactColumns,
    /// What the scan read of a folder that failed validation.
    invalid: Option<folder_scans::InvalidColumns>,
    /// The group the release is listed under, for a member of a group.
    group: Option<MemberRow>,
    /// What the row shows past its placement.
    shown: ShownColumns,
}

/// The columns a release's row shows past its placement, by whether this
/// device imported the release.
enum ShownColumns {
    /// A release this device imported, which shows the library release.
    Imported {
        release: ImportedRelease,
        /// The version of the library release's cover, if it has one.
        cover: Option<String>,
        /// The catalogs the library release's records name.
        records: Vec<ReleaseRecord>,
        /// What the library release's records identify it as, as
        /// [`release_records_kind_sql`] states it.
        records_kind: Option<String>,
        /// How the library release's upload stands.
        upload: Option<UploadProgress>,
    },
    /// A release this device has not imported, which shows the candidate's
    /// draft source, link and cover. They are the candidate's own only while
    /// its state describes its current files.
    Candidate {
        /// The records of what the candidate's link names, merged.
        link_records: Option<String>,
        provenance: Option<StoredProvenance>,
        link: Option<StoredReleaseLink>,
        cover: Option<StoredCover>,
        cover_versions: CoverVersionColumns,
    },
}

impl ShownColumns {
    fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(match row.get::<_, Option<String>>(18)? {
            Some(release_id) => Self::Imported {
                release: ImportedRelease { release_id, album_id: row.get(19)? },
                cover: row.get("library_cover")?,
                records: records_column(row, "library_records")?,
                records_kind: row.get("library_records_kind")?,
                upload: release_upload_progress_in_row(row)?,
            },
            None => Self::Candidate {
                link_records: row.get("link_records")?,
                provenance: read_provenance(row)?,
                link: read_release_link(row)?,
                cover: read_cover(row)?,
                cover_versions: CoverVersionColumns::read(row)?,
            },
        })
    }

    /// The library release a release this device imported shows.
    fn imported(&self) -> Option<&ImportedRelease> {
        match self {
            Self::Imported { release, .. } => Some(release),
            Self::Candidate { .. } => None,
        }
    }
}

/// The statement's last part: each item of the range, a group's header or a
/// release's rows, by its key, in the window's order. A group's header reads
/// its header; a release reads its placement, the keyed rows beside it, the
/// header of its group and whether it ends its group, as `ends_group`
/// reads it.
fn rows_statement(columns: &str, ends_group: &str) -> String {
    use crate::tables::import_state::failure_rows::FAILURE_REASON_COLUMNS;
    let invalid_columns = folder_scans::invalid_columns_sql("c");
    let covers = cover_version_joins(
        "(CASE WHEN origin.release_id IS NULL THEN cover.kind END)",
        "cover.file_id",
    );
    let link_records = claimed_records_sql("link.content_hash");
    format!(
        "SELECT p.watched_folder_path, p.key, \
                p.has_summary, p.shown_title, p.shown_credits, {LISTED_COLUMNS}, \
                c.name, c.content_hash, s.content_hash IS NOT NULL, \
                origin.release_id, released.album_id, \
                g.blocked, g.blocked_subject, g.blocked_holder, \
                failure.content_hash IS NOT NULL AS failed, {FAILURE_REASON_COLUMNS}, \
                {RUNTIME_FACT_COLUMNS}, \
                {library_cover} AS library_cover, \
                {PROVENANCE_COLUMNS}, {RELEASE_LINK_COLUMNS}, {cover}, {invalid}, {actions}, \
                w.item AS item, w.part AS part, {header_columns}, \
                {ends_group} AS ends_group, \
                {RELEASE_UPLOAD_COUNT_COLUMNS}, \
                CASE WHEN origin.release_id IS NOT NULL THEN {library_records} END \
                    AS library_records, \
                {library_records_kind} AS library_records_kind, \
                CASE WHEN link.content_hash IS NOT NULL AND origin.release_id IS NULL \
                     THEN ({link_records}) END AS link_records, \
                {COVER_VERSION_COLUMNS}, {columns} \
         FROM keyed AS w \
         LEFT JOIN import_list_placement AS hp \
             ON hp.watched_folder_path = w.watched_folder_path \
            AND hp.item = 'group' AND hp.key = {HEADER_FOLDER} \
         LEFT JOIN import_list_placement AS p \
             ON w.item <> 'group' \
            AND p.watched_folder_path = w.watched_folder_path \
            AND p.key = w.key AND p.item <> 'group' \
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
         ORDER BY w.n",
        cover = cover_read_columns(),
        library_cover = bae_records::records::files::cover_version_sql("origin.release_id"),
        library_records = release_records_json_sql("origin.release_id"),
        library_records_kind = release_records_kind_sql("origin.release_id"),
        invalid = invalid_columns.0,
        invalid_join = invalid_columns.1,
        actions = action_columns(),
        header_columns = header_columns(),
    )
}

/// One release's columns of a row of [`rows_statement`].
fn read_release(row: &Row<'_>) -> rusqlite::Result<ReleaseRows> {
    use crate::tables::import_state::failure_rows::failure_reason_from_row;
    let summary = match row.get::<_, Option<bool>>(2)? {
        Some(true) => Some(TriageMetadataSummary {
            album_title: row.get(3)?,
            album_artists: crate::tables::stored_credits::line_column(row, "shown_credits")?
                .unwrap_or_default(),
        }),
        Some(false) | None => None,
    };
    let failed: bool = row.get("failed")?;
    Ok(ReleaseRows {
        watched_folder_path: path_column::get(row, 0)?,
        key: row.get(1)?,
        summary,
        listed: ListedColumns::read(row, 5)?,
        name: row.get(15)?,
        block: (row.get(20)?, row.get(21)?, row.get(22)?),
        failure: match failed {
            true => Some(failure_reason_from_row(row)?),
            false => None,
        },
        facts: RuntimeFactColumns::read(row, 25)?,
        invalid: folder_scans::InvalidColumns::read(row)?,
        group: read_member(row)?,
        shown: ShownColumns::read(row)?,
    })
}

/// Chooses what the row of a release this device imported shows from what
/// the library release's records identify: a pressing or an album with those
/// records, or a prefilled row when they identify neither.
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

/// A release's item: a folder that failed validation as its scan read it, or
/// a candidate as its placement and the keyed rows beside it say.
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
    let row = invalid.row(release.key, release.name, release.watched_folder_path)?;
    Ok(ImportListItem::Invalid { row })
}

/// A candidate's row: its placement and the keyed rows beside it, listed
/// under its group's header, if it has one.
fn candidate_item(release: ReleaseRows) -> Result<ImportListItem, DbError> {
    let ReleaseRows {
        watched_folder_path,
        key,
        summary,
        listed,
        name,
        block,
        failure,
        facts,
        group,
        shown,
        ..
    } = release;
    let ListedRelease { basis, actions, standing } = listed.listed(&key)?;
    let facts = facts.facts(&key)?;
    let (blocked, subject, holder) = block;
    let block = block_of(blocked, subject, holder)?;
    let import_status = import_status_of(shown.imported(), block.as_ref(), failure.as_ref());
    let shown = match shown {
        // A release this device imported reads the library release: its
        // cover, the catalogs its records name, and its upload.
        ShownColumns::Imported { release, cover, records, records_kind, upload } => ShownRows {
            metadata_provenance: None,
            release_link: None,
            reading: library_reading(records_kind.as_deref(), records)?,
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
                reading: TriageReading::of(summary.as_ref(), release_link.as_ref(), records),
                release_link,
                cover,
                upload: None,
            }
        }
    };
    let row = TriageRow {
        candidate_key: key,
        folder_name: name,
        watched_folder_path,
        actionable: basis.actionable,
        placement: basis.placement,
        action_basis: basis,
        reading: shown.reading,
        metadata_summary: summary,
        cover: shown.cover,
        import_status,
        upload: shown.upload,
        metadata_provenance: shown.metadata_provenance,
        release_link: shown.release_link,
    };
    Ok(ImportListItem::Candidate {
        row,
        live: crate::CandidateLiveState::of(actions, facts, standing),
        group: group
            .map(|MemberRow { header, last }| {
                Ok::<_, DbError>(ImportGroupMember { header: group_header(header)?, last })
            })
            .transpose()?,
    })
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
