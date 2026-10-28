//! A window of the list, loaded whole: each item the window's keys name, read
//! by key, so a window costs its own rows whatever the list holds.
//!
//! A release's row comes from its placement — what it shows, its state and
//! what its commands are decided from — and the keyed rows beside it: its
//! scan, its link and the records of the stored releases the link names, its
//! draft's source, its cover and what is running for it. None of it opens
//! the folder's files. A group's header says how many of its releases the
//! filters keep, whether the list folds it, and whether its folder is offered
//! to be read as one release, as its placement holds it.

use super::super::folder_scans;
use super::super::import_state::{
    load_covers_on, load_provenance_on, load_release_links_on, RuntimeFactColumns,
    RUNTIME_FACT_COLUMNS,
};
use super::super::query::CandidateRows;
use super::super::release_groupings::block_of;
use super::super::source_releases::load_release_records_on;
use super::listed::{ListedColumns, LISTED_COLUMNS};
use super::order::{Item, ItemKey};
use super::*;
use crate::import::folder_scanner::{FileVersion, ScanItem};
use crate::import::list::{ImportListItem, ListedRelease};
use crate::import::triage::{
    import_status_of, TriageGroup, TriageMetadataSummary, TriageReading, TriageRow,
};
use crate::import::{CoverSelection, FolderReleaseDecisionKey, PressingLink, ReleaseLink};
use std::path::PathBuf;

/// The items `keys` name, in their order, loaded whole for the list `list`,
/// each release with the position of the header `header_positions` lists it
/// under.
pub(super) fn materialise(
    sql: &SqlReadContext<'_>,
    list: &ListRow,
    keys: &[ItemKey],
    header_positions: &[Option<u64>],
) -> Result<Vec<WindowItemRows>, DbError> {
    let mut headers = header_rows(sql, list, keys.iter().filter(|key| key.item == Item::Group))?;
    let mut releases = release_rows(
        sql,
        list.id,
        keys.iter().filter(|key| key.item != Item::Group),
    )?;
    let current: Vec<&str> = releases
        .values()
        .filter(|release| release.current)
        .filter_map(|release| release.content_hash.as_deref())
        .collect();
    let current = CandidateRows::Of(&current);
    let mut provenances = load_provenance_on(sql, current)?;
    let mut links = load_release_links_on(sql, current)?;
    let mut covers = load_covers_on(sql, current)?;
    keys.iter()
        .zip(header_positions)
        .map(|(key, &header_position)| {
            let missing = || {
                DbError::Message(format!(
                    "the list's {:?} at {} vanished while its window was read",
                    key.item, key.key
                ))
            };
            if key.item == Item::Group {
                let header = headers
                    .remove(&(key.watched_folder_path.clone(), key.key.clone()))
                    .ok_or_else(missing)?;
                return Ok(WindowItemRows::Ready(header));
            }
            let release = releases.remove(&key.key).ok_or_else(missing)?;
            if key.item == Item::Invalid {
                return invalid_item(sql, release);
            }
            // The draft's source, the link and the cover are the
            // candidate's own only while its state describes its current
            // files.
            let hash = release.content_hash.clone().filter(|_| release.current);
            let hash = hash.as_ref();
            let provenance = hash.and_then(|hash| provenances.remove(hash));
            let link = hash.and_then(|hash| links.remove(hash));
            let cover = hash.and_then(|hash| covers.remove(hash));
            candidate_item(sql, release, header_position, provenance, link, cover)
        })
        .collect()
}

/// One group header's columns, read for the window that holds it. How many
/// of its releases the filters keep is the counts kept beside the placement,
/// unless a text filter reads the rows; whether its folder is offered to be
/// read as one release is its placement's (`import_list_recombine`).
fn header_rows<'a>(
    sql: &SqlReadContext<'_>,
    list: &ListRow,
    keys: impl Iterator<Item = &'a ItemKey>,
) -> Result<HashMap<(String, String), ImportListItem>, DbError> {
    let heads: Vec<serde_json::Value> = keys
        .map(|key| serde_json::json!([key.watched_folder_path, key.key]))
        .collect();
    if heads.is_empty() {
        return Ok(HashMap::new());
    }
    let entry_count = match list.filters_text {
        true => {
            "(SELECT count(*) FROM import_list_matching AS m \
              WHERE m.list_id = :list AND m.watched_folder_path = h.root \
                AND m.group_name = h.name)"
        }
        false => {
            "(SELECT COALESCE(SUM(g.count), 0) FROM import_list_view AS v \
              JOIN import_list_group_entry_count AS g \
                  ON v.pending_filter = 'all' OR g.pending_entry = v.pending_filter \
              WHERE v.id = :list AND g.watched_folder_path = h.root AND g.group_name = h.name)"
        }
    };
    let rows = sql.query(
        &format!(
            "WITH heads (root, name) AS ( \
                 SELECT value ->> 0, value ->> 1 FROM json_each(:heads)) \
             SELECT h.root, h.name, \
                    NOT EXISTS (SELECT 1 FROM import_list_collapsed AS c \
                                WHERE c.list_id = :list AND c.watched_folder_path = h.root \
                                  AND c.group_name = h.name) AS expanded, \
                    {entry_count} AS entry_count, p.combinable \
             FROM heads AS h \
             JOIN import_list_placement AS p \
                 ON p.watched_folder_path = h.root AND p.item = 'group' AND p.key = h.name"
        ),
        named_params! {
            ":heads": serde_json::Value::Array(heads).to_string(),
            ":list": list.id.0,
        },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, bool>(4)?,
            ))
        },
    )?;
    rows.into_iter()
        .map(|(root, name, expanded, entry_count, combinable)| {
            let header = ImportListItem::GroupHeader {
                group: TriageGroup {
                    key: FolderReleaseDecisionKey {
                        watched_folder_path: root.clone(),
                        relative_folder_path: name.clone(),
                    },
                    name: name.clone(),
                    combinable,
                },
                watched_folder_path: root.clone(),
                expanded,
                entry_count: to_u32(entry_count, "a group's entry count")?,
            };
            Ok(((root, name), header))
        })
        .collect()
}

/// One release's columns, read for the window that holds it.
#[derive(Clone, PartialEq)]
struct ReleaseRows {
    watched_folder_path: String,
    key: String,
    display_path: String,
    /// What the row shows: its draft's title and artists while it shows its
    /// draft, else nothing past its folder's name.
    summary: Option<TriageMetadataSummary>,
    listed: ListedColumns,
    name: String,
    content_hash: Option<String>,
    /// Whether the candidate's state describes its current files, which is
    /// when its draft, link, lookup and cover are its own.
    current: bool,
    imported: Option<ImportedRelease>,
    block: (Option<String>, Option<String>, Option<String>),
    failure: Option<crate::import::ImportFailureReason>,
    facts: RuntimeFactColumns,
    selected: bool,
}

fn release_rows<'a>(
    sql: &SqlReadContext<'_>,
    list: ImportListId,
    keys: impl Iterator<Item = &'a ItemKey>,
) -> Result<HashMap<String, ReleaseRows>, DbError> {
    use super::super::import_state::failure_rows::{
        failure_reason_from_row, FAILURE_REASON_COLUMNS,
    };
    let keys: Vec<&str> = keys.map(|key| key.key.as_str()).collect();
    if keys.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sql.query(
        &format!(
            "SELECT p.watched_folder_path, p.key, p.display_path, \
                    p.has_summary, p.shown_title, p.shown_artists, {LISTED_COLUMNS}, \
                    c.name, c.content_hash, s.content_hash IS NOT NULL, \
                    origin.release_id, released.album_id, \
                    g.blocked, g.blocked_subject, g.blocked_holder, \
                    failure.content_hash IS NOT NULL AS failed, {FAILURE_REASON_COLUMNS}, \
                    {RUNTIME_FACT_COLUMNS}, \
                    EXISTS (SELECT 1 FROM import_list_selection AS chosen \
                            WHERE chosen.list_id = :list AND chosen.key = p.key) AS selected \
             FROM json_each(:keys) AS w \
             JOIN import_list_placement AS p ON p.key = w.value AND p.item <> 'group' \
             JOIN scan_candidate AS c \
                 ON c.watched_folder_path = p.watched_folder_path AND c.path = p.key \
             LEFT JOIN release_grouping AS g ON g.key = c.grouping_key \
             LEFT JOIN import_candidate_state AS s \
                 ON s.content_hash = c.content_hash AND s.edit_revision = c.file_edit_revision \
             LEFT JOIN local_release_origin AS origin ON origin.content_hash = c.content_hash \
             LEFT JOIN releases AS released ON released.id = origin.release_id \
             LEFT JOIN import_candidate_failure AS failure \
                 ON failure.content_hash = c.content_hash \
             LEFT JOIN import_candidate_standing AS st ON st.candidate_key = p.key"
        ),
        named_params! {
            ":keys": serde_json::Value::from(keys).to_string(),
            ":list": list.0,
        },
        |row| {
            let summary = match row.get::<_, Option<bool>>(3)? {
                Some(true) => Some(TriageMetadataSummary {
                    album_title: row.get(4)?,
                    album_artists: row.get(5)?,
                }),
                Some(false) | None => None,
            };
            let failed: bool = row.get("failed")?;
            Ok(ReleaseRows {
                watched_folder_path: row.get(0)?,
                key: row.get(1)?,
                display_path: row.get(2)?,
                summary,
                listed: ListedColumns::read(row, 6)?,
                name: row.get(17)?,
                content_hash: row.get(18)?,
                current: row.get(19)?,
                imported: match row.get::<_, Option<String>>(20)? {
                    Some(release_id) => Some(ImportedRelease {
                        release_id,
                        album_id: row.get(21)?,
                    }),
                    None => None,
                },
                block: (row.get(22)?, row.get(23)?, row.get(24)?),
                failure: match failed {
                    true => Some(failure_reason_from_row(row)?),
                    false => None,
                },
                facts: RuntimeFactColumns::read(row, 27)?,
                selected: row.get("selected")?,
            })
        },
    )?;
    Ok(rows
        .into_iter()
        .map(|release| (release.key.clone(), release))
        .collect())
}

/// A folder that failed validation, as its row shows it: what its scan read.
fn invalid_item(sql: &SqlReadContext<'_>, release: ReleaseRows) -> Result<WindowItemRows, DbError> {
    // Only a row inside a window shows what its files are, so only such a
    // row's files are read.
    let stored = folder_scans::load_item_by_key(sql, &release.key, folder_scans::RowSources::Any)?;
    let candidate = match stored.map(|(_, stored)| stored.item) {
        Some(ScanItem::Invalid(candidate)) => candidate,
        _ => {
            return Err(DbError::Message(format!(
                "the folder {} that failed validation vanished while its row was read",
                release.key
            )))
        }
    };
    Ok(WindowItemRows::Ready(ImportListItem::Invalid {
        candidate,
        selected: release.selected,
        is_group_member: false,
    }))
}

/// A release's row: its placement and the keyed rows beside it, listed under
/// the header at `header_position`.
fn candidate_item(
    sql: &SqlReadContext<'_>,
    release: ReleaseRows,
    header_position: Option<u64>,
    metadata_provenance: Option<crate::import::MetadataProvenance>,
    release_link: Option<ReleaseLink>,
    cover: Option<CoverSelection>,
) -> Result<WindowItemRows, DbError> {
    let ListedRelease { basis, standing } = release.listed.clone().listed(&release.key)?;
    let facts = release.facts.facts(&release.key)?;
    let (blocked, subject, holder) = release.block.clone();
    let block = block_of(blocked, subject, holder)?;
    let import_status = import_status_of(
        release.imported.as_ref(),
        block.as_ref(),
        release.failure.as_ref(),
    );
    // A pressing's releases' records are read in this snapshot and put
    // together after it ends; an album names its records itself.
    let picked = match release_link.as_ref() {
        Some(ReleaseLink::Pressing(pressing)) => {
            Some(LinkedRows::Pressing(claimed_records(sql, pressing)?))
        }
        Some(ReleaseLink::Album(album)) => Some(LinkedRows::Album(album.records())),
        None => None,
    };
    let cover = cover
        .as_ref()
        .map(|cover| row_cover_source(sql, &release.watched_folder_path, &release.key, cover))
        .transpose()?;
    let row = TriageRow {
        candidate_key: release.key.clone(),
        folder_name: release.name.clone(),
        watched_folder_path: release.watched_folder_path.clone(),
        display_path: release.display_path.clone(),
        actionable: basis.actionable,
        placement: basis.placement,
        action_basis: basis.clone(),
        reading: TriageReading::of(release.summary.as_ref(), release_link.as_ref(), Vec::new()),
        metadata_summary: release.summary.clone(),
        cover: None,
        import_status,
        metadata_provenance,
        release_link,
        selected: release.selected,
    };
    Ok(WindowItemRows::Candidate {
        row,
        live: crate::import::CandidateLiveState::of(&basis, facts, standing),
        picked,
        cover,
        header_position,
    })
}

#[derive(Clone, PartialEq)]
pub(super) enum WindowItemRows {
    Ready(ImportListItem),
    Candidate {
        row: TriageRow,
        live: crate::import::CandidateLiveState,
        picked: Option<LinkedRows>,
        /// The candidate's stored cover selection, as the row draws it.
        /// Nothing stands in for an empty one: the row shows what the
        /// candidate would commit with.
        cover: Option<crate::import::CoverImageSource>,
        header_position: Option<u64>,
    },
}

impl WindowItemRows {
    pub(super) fn process(self) -> Result<ImportListItem, DbError> {
        match self {
            Self::Ready(item) => Ok(item),
            Self::Candidate {
                mut row,
                live,
                picked,
                cover,
                header_position,
            } => {
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
                Ok(ImportListItem::Candidate {
                    row,
                    live,
                    header_position,
                })
            }
        }
    }
}

/// What a linked row's link reads: the records of each release a pressing
/// claims, the primary's first and then its partners', each release's own
/// record first; or an album's records, which the link names itself.
#[derive(Clone, PartialEq)]
pub(super) enum LinkedRows {
    Pressing(Vec<Vec<crate::import::ReleaseRecord>>),
    Album(Vec<crate::import::ReleaseRecord>),
}

impl LinkedRows {
    /// Every catalog that describes what the row is linked to.
    fn records(self) -> Vec<crate::import::ReleaseRecord> {
        match self {
            Self::Pressing(claimed) => crate::import::source_release::claimed_records(&claimed),
            Self::Album(records) => records,
        }
    }
}

/// The image the row of the candidate at `candidate_path` draws for `cover`.
fn row_cover_source(
    sql: &SqlReadContext<'_>,
    watched_folder_path: &str,
    candidate_path: &str,
    cover: &CoverSelection,
) -> Result<crate::import::CoverImageSource, DbError> {
    match cover {
        CoverSelection::Remote(image, _) => Ok(crate::import::CoverImageSource::Remote {
            image: image.clone(),
        }),
        CoverSelection::Local(file_id) => Ok(crate::import::CoverImageSource::Local {
            file: scanned_file_version(sql, watched_folder_path, candidate_path, file_id)?,
        }),
        CoverSelection::Embedded(source_file_id) => {
            let (source, data) =
                stored_embedded_cover(sql, watched_folder_path, candidate_path, source_file_id)?;
            Ok(crate::import::CoverImageSource::Embedded { source, data })
        }
    }
}

/// The version of one of the candidate's files that its scan read.
fn scanned_file_version(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    candidate_path: &str,
    file_id: &str,
) -> Result<FileVersion, DbError> {
    let (path, size, modified_at_ns) = sql
        .query_row(
            "SELECT absolute_path, size, modified_at_ns FROM scan_candidate_file \
             WHERE watched_folder_path = ? AND candidate_path = ? \
               AND relative_path = ?",
            params![watched_folder_path, candidate_path, file_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            DbError::Message(format!(
                "candidate {candidate_path} names missing file {file_id}"
            ))
        })?;
    Ok(FileVersion {
        path: PathBuf::from(path),
        size: to_u64(size, "a scanned file's size")?,
        modified_at_ns,
    })
}

/// The embedded cover the candidate selects from `source_file_id`: the bytes
/// its file-tag snapshot stored, and the version of the source file the
/// snapshot read them from.
pub(super) fn stored_embedded_cover(
    sql: &SqlReadContext<'_>,
    watched_folder_path: &str,
    candidate_path: &str,
    source_file_id: &str,
) -> Result<(FileVersion, Vec<u8>), DbError> {
    type CoverColumns = (
        Option<String>,
        Option<Vec<u8>>,
        Option<i64>,
        Option<i64>,
        Option<String>,
    );
    let (stored_source, data, size, modified_at_ns, path): CoverColumns = sql
        .query_row(
            "SELECT snapshot.embedded_cover_source_relative_path, snapshot.embedded_cover_data, \
                    tags.file_size, tags.modified_at_ns, files.absolute_path \
             FROM scan_candidate_tag_snapshot AS snapshot \
             LEFT JOIN scan_candidate_file_tag AS tags \
               ON tags.watched_folder_path = snapshot.watched_folder_path \
              AND tags.candidate_path = snapshot.candidate_path \
              AND tags.relative_path = :source \
             LEFT JOIN scan_candidate_file AS files \
               ON files.watched_folder_path = snapshot.watched_folder_path \
              AND files.candidate_path = snapshot.candidate_path \
              AND files.relative_path = :source \
             WHERE snapshot.watched_folder_path = :root AND snapshot.candidate_path = :path",
            rusqlite::named_params! {
                ":root": watched_folder_path,
                ":path": candidate_path,
                ":source": source_file_id,
            },
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            DbError::Message(format!(
                "candidate {candidate_path} selects embedded cover without a file-tag snapshot"
            ))
        })?;
    let (Some(stored_source), Some(data)) = (stored_source, data) else {
        return Err(DbError::Message(format!(
            "candidate {candidate_path} selects embedded cover without stored artwork"
        )));
    };
    if stored_source != source_file_id {
        return Err(DbError::Message(format!(
            "candidate {candidate_path} selects embedded cover from {source_file_id}, but its snapshot stores {stored_source}"
        )));
    }
    let (Some(size), Some(modified_at_ns)) = (size, modified_at_ns) else {
        return Err(DbError::Message(format!(
            "candidate {candidate_path}'s snapshot stores a cover from {source_file_id} but no reading of that file"
        )));
    };
    let path = path.ok_or_else(|| {
        DbError::Message(format!(
            "candidate {candidate_path} names missing file {source_file_id}"
        ))
    })?;
    Ok((
        FileVersion {
            path: PathBuf::from(path),
            size: to_u64(size, "a file-tag reading's size")?,
            modified_at_ns,
        },
        data,
    ))
}

/// The records of every stored release a linked pressing claims, the
/// primary's first.
fn claimed_records(
    sql: &SqlReadContext<'_>,
    link: &PressingLink,
) -> Result<Vec<Vec<crate::import::ReleaseRecord>>, DbError> {
    link.claimed()
        .map(|release| {
            load_release_records_on(sql, release)?.ok_or_else(|| {
                DbError::Message(format!(
                    "{} release {} is linked but nothing stored it",
                    release.catalog.as_str(),
                    release.key
                ))
            })
        })
        .collect()
}
