//! A window of the list, loaded whole: each item the window's keys name, read
//! by key, so a window costs its own rows whatever the list holds.
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

use super::listed::{ListedColumns, LISTED_COLUMNS};
use super::order::{Item, ItemKey};
use super::*;
use crate::candidate_detail::ListedRelease;
use crate::folder_scanner::ScanItem;
use crate::list::ImportListItem;
use crate::tables::folder_scans;
use crate::tables::import_state::{
    cover_read_columns, read_cover, read_provenance, read_release_link, RuntimeFactColumns,
    PROVENANCE_COLUMNS, RELEASE_LINK_COLUMNS, RUNTIME_FACT_COLUMNS,
};
use crate::tables::release_groupings::{block_of, release_of_stored};
use crate::tables::source_releases::load_releases_records_on;
use crate::triage::{
    import_status_of, TriageGroup, TriageMetadataSummary, TriageReading, TriageRow,
};
use crate::RelativeFolder;
use crate::{CandidateKey, CoverSelection, FolderReleaseDecisionKey, ImportedRelease, ReleaseLink};
use bae_catalogs::records::{LookupRef, ReleaseRecord};
use bae_identify::PressingLink;
use bae_records::records::files::cover_version_sql;
use bae_records::records::queued_uploads::release_upload_progress_on;
use bae_records::records::read::get_releases_records_on;
use bae_records::UploadProgress;
use std::collections::HashMap;
use std::path::PathBuf;

/// The items `keys` name, in their order, loaded whole for the list `list`,
/// each release with the position of the header `header_positions` lists it
/// under. The headers and the releases are each read in the window's order,
/// so each item is the next row of its kind.
pub(super) fn materialise(
    sql: &SqlReadContext<'_>,
    list: &ListRow,
    keys: &[ItemKey],
    header_positions: &[Option<u64>],
) -> Result<Vec<WindowItemRows>, DbError> {
    let mut headers = header_rows(sql, list, keys)?.into_iter().peekable();
    let releases = release_rows(sql, list.id, keys)?;
    let imported = ImportedReads::of(sql, &releases)?;
    let linked = LinkedReads::of(sql, keys, &releases)?;
    let mut scanned = scanned_items(sql, keys, &releases)?;
    let mut covers = row_covers(sql, keys, &releases, &mut scanned)?;
    let mut releases = releases.into_iter().peekable();
    keys.iter()
        .enumerate()
        .zip(header_positions)
        .map(|((position, key), &header_position)| {
            let missing = || {
                // The window's keys and rows are read in one snapshot.
                DbError::unreadable(
                    format!("the rows of the list's {:?} at {}", key.item, key.shown()),
                    0,
                )
            };
            if key.item == Item::Group {
                let (_, header) = headers.next_if(|(at, _)| *at == position).ok_or_else(missing)?;
                return Ok(WindowItemRows::Ready(header));
            }
            let release =
                releases.next_if(|release| release.position == position).ok_or_else(missing)?;
            if key.item == Item::Invalid {
                return invalid_item(&mut scanned, release);
            }
            candidate_item(&imported, &linked, &mut covers, release, header_position)
        })
        .collect()
}

/// One group header's columns, read for the window that holds it. How many
/// of its releases the filters keep is the counts kept beside the placement,
/// unless a text filter reads the rows; whether its folder is offered to be
/// read as one release is its placement's (`import_list_recombine`).
fn header_rows(
    sql: &SqlReadContext<'_>,
    list: &ListRow,
    keys: &[ItemKey],
) -> Result<Vec<(usize, ImportListItem)>, DbError> {
    let heads: Vec<serde_json::Value> = keys
        .iter()
        .enumerate()
        .filter(|(_, key)| key.item == Item::Group)
        .map(|(position, key)| {
            serde_json::json!([
                position,
                hex::encode(&key.watched_folder_path),
                hex::encode(&key.key)
            ])
        })
        .collect();
    if heads.is_empty() {
        return Ok(Vec::new());
    }
    let entry_count = match list.filters_text {
        true => {
            "(SELECT count(*) FROM import_list_matching AS m \
              WHERE m.list_id = :list AND m.watched_folder_path = h.root \
                AND m.group_folder = h.folder)"
        }
        false => {
            "(SELECT COALESCE(SUM(g.count), 0) FROM import_list_view AS v \
              JOIN import_list_group_entry_count AS g \
                  ON v.pending_filter = 'all' OR g.pending_entry = v.pending_filter \
              WHERE v.id = :list AND g.watched_folder_path = h.root AND g.group_folder = h.folder)"
        }
    };
    let rows = sql.query(
        &format!(
            "WITH heads (position, root, folder) AS ( \
                 SELECT value ->> 0, unhex(value ->> 1), unhex(value ->> 2) \
                 FROM json_each(:heads)) \
             SELECT h.position, h.root, h.folder, p.group_name, \
                    NOT EXISTS (SELECT 1 FROM import_list_collapsed AS c \
                                WHERE c.list_id = :list AND c.watched_folder_path = h.root \
                                  AND c.group_folder = h.folder) AS expanded, \
                    {entry_count} AS entry_count, p.combinable \
             FROM heads AS h \
             JOIN import_list_placement AS p \
                 ON p.watched_folder_path = h.root AND p.item = 'group' AND p.key = h.folder \
             ORDER BY h.position"
        ),
        named_params! {
            ":heads": serde_json::Value::Array(heads).to_string(),
            ":list": list.id.0,
        },
        |row| {
            Ok((
                row.get::<_, usize>(0)?,
                path_column::get(row, 1)?,
                row.get::<_, RelativeFolder>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, bool>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, bool>(6)?,
            ))
        },
    )?;
    rows.into_iter()
        .map(
            |(position, root, folder, name, expanded, entry_count, combinable): (
                usize,
                PathBuf,
                RelativeFolder,
                String,
                bool,
                i64,
                bool,
            )| {
                let header = ImportListItem::GroupHeader {
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
                };
                Ok((position, header))
            },
        )
        .collect()
}

/// One release's columns, read for the window that holds it.
#[derive(Clone, PartialEq)]
struct ReleaseRows {
    /// Where in the window the release is.
    position: usize,
    watched_folder_path: PathBuf,
    key: CandidateKey,
    display_path: String,
    /// What the row shows: an imported release's library album title and
    /// credits, else its draft's title and artists while it shows its draft,
    /// else nothing past its folder's name.
    summary: Option<TriageMetadataSummary>,
    listed: ListedColumns,
    name: String,
    content_hash: Option<String>,
    /// Whether the candidate's state describes its current files, which is
    /// when its draft, link, lookup and cover are its own.
    current: bool,
    /// Whether that state holds a draft row; one that holds none holds its
    /// files' blank draft and the folder's own cover.
    drafted: bool,
    imported: Option<ImportedRelease>,
    /// The version of the imported release's library cover, if it has one.
    library_cover: Option<String>,
    block: (Option<String>, Option<String>, Option<String>),
    failure: Option<crate::ImportFailureReason>,
    facts: RuntimeFactColumns,
    selected: bool,
    /// The draft's source, the link and the cover, which are the
    /// candidate's own only while its state describes its current files.
    metadata_provenance: Option<crate::MetadataProvenance>,
    release_link: Option<ReleaseLink>,
    cover: Option<CoverSelection>,
}

impl ReleaseRows {
    /// Whether the row is a current candidate this device did not import
    /// that holds no draft row, so it shows the folder's own cover its blank
    /// draft comes with, read from its scan.
    fn reads_blank_cover(&self) -> bool {
        self.imported.is_none() && self.current && !self.drafted
    }
}

/// The releases of the window `keys`, in its order.
fn release_rows(
    sql: &SqlReadContext<'_>,
    list: ImportListId,
    keys: &[ItemKey],
) -> Result<Vec<ReleaseRows>, DbError> {
    use crate::tables::import_state::failure_rows::{
        failure_reason_from_row, FAILURE_REASON_COLUMNS,
    };
    let keys: Vec<serde_json::Value> = keys
        .iter()
        .enumerate()
        .filter(|(_, key)| key.item != Item::Group)
        .map(|(position, key)| serde_json::json!([position, hex::encode(&key.key)]))
        .collect();
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sql.query(
        &format!(
            "SELECT p.watched_folder_path, p.key, p.display_path, \
                    p.has_summary, p.shown_title, p.shown_credits, {LISTED_COLUMNS}, \
                    c.name, c.content_hash, s.content_hash IS NOT NULL, \
                    origin.release_id, released.album_id, \
                    g.blocked, g.blocked_subject, g.blocked_holder, \
                    failure.content_hash IS NOT NULL AS failed, {FAILURE_REASON_COLUMNS}, \
                    {RUNTIME_FACT_COLUMNS}, \
                    EXISTS (SELECT 1 FROM import_list_selection AS chosen \
                            WHERE chosen.list_id = :list AND chosen.key = p.key) AS selected, \
                    w.value ->> 0 AS window_position, {library_cover} AS library_cover, \
                    EXISTS (SELECT 1 FROM import_candidate_edit AS drafted \
                            WHERE drafted.content_hash = s.content_hash) AS drafted, \
                    {PROVENANCE_COLUMNS}, {RELEASE_LINK_COLUMNS}, {cover} \
             FROM json_each(:keys) AS w \
             JOIN import_list_placement AS p \
                 ON p.key = unhex(w.value ->> 1) AND p.item <> 'group' \
             JOIN scan_candidate AS c \
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
             ORDER BY window_position",
            cover = cover_read_columns(),
            library_cover = cover_version_sql("origin.release_id"),
        ),
        named_params! {
            ":keys": serde_json::Value::from(keys).to_string(),
            ":list": list.0,
        },
        |row| {
            let summary = match row.get::<_, Option<bool>>(3)? {
                Some(true) => Some(TriageMetadataSummary {
                    album_title: row.get(4)?,
                    album_artists: crate::tables::stored_credits::line_column(
                        row,
                        "shown_credits",
                    )?
                    .unwrap_or_default(),
                }),
                Some(false) | None => None,
            };
            let failed: bool = row.get("failed")?;
            let release = ReleaseRows {
                position: row.get("window_position")?,
                watched_folder_path: path_column::get(row, 0)?,
                key: row.get(1)?,
                display_path: row.get(2)?,
                summary,
                listed: ListedColumns::read(row, 6)?,
                name: row.get(16)?,
                content_hash: row.get(17)?,
                current: row.get(18)?,
                imported: match row.get::<_, Option<String>>(19)? {
                    Some(release_id) => {
                        Some(ImportedRelease { release_id, album_id: row.get(20)? })
                    }
                    None => None,
                },
                library_cover: row.get("library_cover")?,
                drafted: row.get("drafted")?,
                block: (row.get(21)?, row.get(22)?, row.get(23)?),
                failure: match failed {
                    true => Some(failure_reason_from_row(row)?),
                    false => None,
                },
                facts: RuntimeFactColumns::read(row, 26)?,
                selected: row.get("selected")?,
                metadata_provenance: None,
                release_link: None,
                cover: None,
            };
            Ok((release, read_provenance(row)?, read_release_link(row)?, read_cover(row)?))
        },
    )?;
    rows.into_iter()
        .map(|(release, provenance, link, cover)| {
            Ok(ReleaseRows {
                metadata_provenance: provenance.map(|stored| stored.provenance()).transpose()?,
                release_link: link.map(|stored| stored.link()).transpose()?,
                cover: cover.map(|stored| stored.selection()).transpose()?,
                ..release
            })
        })
        .collect()
}

/// The scan items the window's rows read their files from, by key, read in
/// one statement: a folder that failed validation shows what its scan read,
/// and a current candidate holding no draft row shows its files' blank
/// cover. Only such a row reads its scan's files.
fn scanned_items(
    sql: &SqlReadContext<'_>,
    keys: &[ItemKey],
    releases: &[ReleaseRows],
) -> Result<HashMap<CandidateKey, folder_scans::StoredScanItem>, DbError> {
    let wanted: Vec<(&std::path::Path, &CandidateKey)> = releases
        .iter()
        .filter(|release| {
            keys[release.position].item == Item::Invalid || release.reads_blank_cover()
        })
        .map(|release| (release.watched_folder_path.as_path(), &release.key))
        .collect();
    folder_scans::load_items_by_keys(sql, &wanted, folder_scans::RowSources::Any)
}

/// A folder that failed validation, as its row shows it: what its scan read.
fn invalid_item(
    scanned: &mut HashMap<CandidateKey, folder_scans::StoredScanItem>,
    release: ReleaseRows,
) -> Result<WindowItemRows, DbError> {
    let candidate = match scanned.remove(&release.key).map(|stored| stored.item) {
        Some(ScanItem::Invalid(candidate)) => candidate,
        stored => {
            return Err(DbError::unreadable(
                format!("the scan item of {}, listed as failing validation", release.key),
                stored.map(|item| matches!(item, ScanItem::Invalid(_))),
            ))
        }
    };
    Ok(WindowItemRows::Ready(ImportListItem::Invalid { candidate, selected: release.selected }))
}

/// What the window's rows with an imported release read of the library
/// beside them: how each release's upload stands and the catalogs its records
/// name, each read once for all the window's imported releases.
struct ImportedReads {
    uploads: HashMap<String, UploadProgress>,
    records: HashMap<String, Vec<bae_catalogs::records::ReleaseRecord>>,
}

impl ImportedReads {
    fn of(sql: &SqlReadContext<'_>, releases: &[ReleaseRows]) -> Result<Self, DbError> {
        let release_ids: Vec<String> = releases
            .iter()
            .filter_map(|release| release.imported.as_ref())
            .map(|imported| imported.release_id.clone())
            .collect();
        // A window of rows that have none reruns on none of the upload
        // queue's changes.
        if release_ids.is_empty() {
            return Ok(Self { uploads: HashMap::new(), records: HashMap::new() });
        }
        Ok(Self {
            uploads: release_upload_progress_on(sql, &release_ids)?,
            records: release_ids
                .iter()
                .cloned()
                .zip(get_releases_records_on(sql, &release_ids)?)
                .collect(),
        })
    }
}

/// A release's row: its placement and the keyed rows beside it, listed under
/// the header at `header_position`.
fn candidate_item(
    imported_reads: &ImportedReads,
    linked_reads: &LinkedReads,
    covers: &mut HashMap<usize, crate::CoverImageSource>,
    release: ReleaseRows,
    header_position: Option<u64>,
) -> Result<WindowItemRows, DbError> {
    let ListedRelease { basis, standing } = release.listed.clone().listed(&release.key)?;
    let facts = release.facts.facts(&release.key)?;
    let (blocked, subject, holder) = release.block.clone();
    let block = block_of(blocked, subject, holder)?;
    let import_status =
        import_status_of(release.imported.as_ref(), block.as_ref(), release.failure.as_ref());
    let upload = release
        .imported
        .as_ref()
        .and_then(|imported| imported_reads.uploads.get(&imported.release_id))
        .cloned();
    let shown = match release.imported.as_ref() {
        // A release this device imported reads the library release: its
        // cover, and the catalogs its records name.
        Some(imported) => ShownRows {
            metadata_provenance: None,
            release_link: None,
            reading: TriageReading::of_library(
                imported_reads.records.get(&imported.release_id).cloned().unwrap_or_default(),
            ),
            picked: None,
            cover: bae_records::cover_ref(&imported.release_id, release.library_cover.clone())
                .map(|image| crate::CoverImageSource::Library { image }),
        },
        None => {
            let release_link = release.release_link.clone();
            // A pressing's releases' records are read in this snapshot and
            // put together after it ends; an album names its records itself.
            let picked = match release_link.as_ref() {
                Some(ReleaseLink::Pressing(pressing)) => {
                    Some(LinkedRows::Pressing(linked_reads.claimed_records(pressing)))
                }
                Some(ReleaseLink::Album(album)) => Some(LinkedRows::Album(album.records())),
                None => None,
            };
            ShownRows {
                metadata_provenance: release.metadata_provenance.clone(),
                reading: TriageReading::of(
                    release.summary.as_ref(),
                    release_link.as_ref(),
                    Vec::new(),
                ),
                release_link,
                picked,
                cover: covers.remove(&release.position),
            }
        }
    };
    let row = TriageRow {
        candidate_key: release.key.clone(),
        folder_name: release.name.clone(),
        watched_folder_path: release.watched_folder_path.clone(),
        display_path: release.display_path.clone(),
        actionable: basis.actionable,
        placement: basis.placement,
        action_basis: basis.clone(),
        reading: shown.reading,
        metadata_summary: release.summary.clone(),
        cover: None,
        import_status,
        upload,
        metadata_provenance: shown.metadata_provenance,
        release_link: shown.release_link,
        selected: release.selected,
    };
    Ok(WindowItemRows::Candidate {
        row,
        live: crate::CandidateLiveState::of(&basis, facts, standing),
        picked: shown.picked,
        cover: shown.cover,
        header_position,
    })
}

/// What a release's row shows past its placement: the candidate's draft
/// source, link and cover, or for an imported release the library
/// release's.
struct ShownRows {
    metadata_provenance: Option<crate::MetadataProvenance>,
    release_link: Option<ReleaseLink>,
    reading: TriageReading,
    picked: Option<LinkedRows>,
    cover: Option<crate::CoverImageSource>,
}

#[derive(Clone, PartialEq)]
pub(super) enum WindowItemRows {
    Ready(ImportListItem),
    Candidate {
        row: TriageRow,
        live: crate::CandidateLiveState,
        picked: Option<LinkedRows>,
        /// The candidate's stored cover selection, as the row draws it, or
        /// an imported release's library cover. Nothing stands in for an
        /// empty one: the row shows what the candidate would commit with, or
        /// what the library release has.
        cover: Option<crate::CoverImageSource>,
        header_position: Option<u64>,
    },
}

impl WindowItemRows {
    pub(super) fn process(self) -> Result<ImportListItem, DbError> {
        match self {
            Self::Ready(item) => Ok(item),
            Self::Candidate { mut row, live, picked, cover, header_position } => {
                // The reading the row was placed with named no records;
                // this is where the linked releases' records are put
                // together.
                if let Some(picked) = picked {
                    row.reading = TriageReading::of(
                        row.metadata_summary.as_ref(),
                        row.release_link.as_ref(),
                        picked.records(),
                    );
                }
                row.cover = cover;
                Ok(ImportListItem::Candidate { row, live, header_position })
            }
        }
    }
}

/// What a linked row's link reads: the records of each release a pressing
/// claims, the primary's first and then its partners', each release's own
/// record first; or an album's records, which the link names itself.
#[derive(Clone, PartialEq)]
pub(super) enum LinkedRows {
    Pressing(Vec<Vec<bae_catalogs::records::ReleaseRecord>>),
    Album(Vec<bae_catalogs::records::ReleaseRecord>),
}

impl LinkedRows {
    /// Every catalog that describes what the row is linked to.
    fn records(self) -> Vec<bae_catalogs::records::ReleaseRecord> {
        match self {
            Self::Pressing(claimed) => bae_catalogs::source_release::claimed_records(&claimed),
            Self::Album(records) => records,
        }
    }
}

/// The cover each row of a candidate this device did not import draws, by
/// the row's position in the window: its stored selection, or, for a state
/// holding no draft row, the folder's own cover its blank draft comes with,
/// read from its scan, which it takes out of `scanned`. The versions of the files the selections
/// name are read together.
fn row_covers(
    sql: &SqlReadContext<'_>,
    keys: &[ItemKey],
    releases: &[ReleaseRows],
    scanned: &mut HashMap<CandidateKey, folder_scans::StoredScanItem>,
) -> Result<HashMap<usize, crate::CoverImageSource>, DbError> {
    let mut selections: Vec<(&ReleaseRows, CoverSelection)> = Vec::new();
    for release in releases {
        if keys[release.position].item == Item::Invalid || release.imported.is_some() {
            continue;
        }
        let selection = if release.reads_blank_cover() {
            let unreadable = || {
                DbError::unreadable(format!("the scan_candidate rows of listed {}", release.key), 0)
            };
            let stored = scanned.remove(&release.key).ok_or_else(unreadable)?;
            let (candidate, _) =
                release_of_stored(&release.watched_folder_path, stored)?.ok_or_else(unreadable)?;
            candidate.blank_cover().as_ref().map(crate::cover_art::PreparedCover::selection)
        } else {
            release.cover.clone()
        };
        if let Some(selection) = selection {
            selections.push((release, selection));
        }
    }
    let wanted: Vec<_> = selections
        .iter()
        .map(|(release, selection)| {
            (release.watched_folder_path.as_path(), &release.key, selection)
        })
        .collect();
    let choices = folder_scans::stored_cover_choices(sql, &wanted)?;
    Ok(selections
        .iter()
        .zip(choices)
        .map(|((release, _), choice)| (release.position, choice.into_image()))
        .collect())
}

/// What the window's rows linked to a pressing read of the stored releases
/// their links claim: the records each release names, read once for every
/// release the window's links claim.
struct LinkedReads {
    records: HashMap<LookupRef, Vec<ReleaseRecord>>,
}

impl LinkedReads {
    fn of(
        sql: &SqlReadContext<'_>,
        keys: &[ItemKey],
        releases: &[ReleaseRows],
    ) -> Result<Self, DbError> {
        let mut claimed: Vec<&LookupRef> = Vec::new();
        for release in releases {
            // A folder that failed validation shows its scan, and a release
            // this device imported the library release, in its link's place.
            if keys[release.position].item == Item::Invalid || release.imported.is_some() {
                continue;
            }
            let Some(ReleaseLink::Pressing(pressing)) = &release.release_link else {
                continue;
            };
            for release in pressing.claimed() {
                if !claimed.contains(&release) {
                    claimed.push(release);
                }
            }
        }
        let records = load_releases_records_on(sql, &claimed)?;
        Ok(Self {
            records: claimed
                .into_iter()
                .zip(records)
                .map(|(release, records)| {
                    let records = records.ok_or_else(|| {
                        DbError::unreadable(
                            format!(
                                "the records of linked {} release {}",
                                release.catalog.as_str(),
                                release.key
                            ),
                            None::<String>,
                        )
                    })?;
                    Ok((release.clone(), records))
                })
                .collect::<Result<_, DbError>>()?,
        })
    }

    /// The records of every stored release `link` claims, the primary's
    /// first.
    fn claimed_records(&self, link: &PressingLink) -> Vec<Vec<ReleaseRecord>> {
        link.claimed()
            .map(|release| {
                self.records.get(release).expect("every release a link claims was read").clone()
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "window_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "row_kinds_tests.rs"]
mod row_kinds_tests;
