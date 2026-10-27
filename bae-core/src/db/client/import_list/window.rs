//! A window of the list, loaded whole: each item the window's keys name, read
//! by key, so a window costs its own rows whatever the list holds.
//!
//! A release's row comes from its placement — what it shows, its state and
//! what its commands are decided from — and the keyed rows beside it: its
//! scan, its stored lookup's lead, its link, its draft's source, its cover
//! and what is running for it. A group's header says how many of its
//! releases the filters keep, whether the list folds it, and whether its
//! folder is offered to be read as one release.

use super::super::folder_scans;
use super::super::import_state::{
    load_covers_on, load_matches_on, load_provenance_on, load_release_links_on, RuntimeFactColumns,
    RUNTIME_FACT_COLUMNS,
};
use super::super::query::CandidateRows;
use super::super::release_groupings::{block_of, load_candidate_on};
use super::super::source_releases::load_source_release_on;
use super::listed::{ListedColumns, LISTED_COLUMNS};
use super::order::{Item, ItemKey};
use super::*;
use crate::identify::LeadMatch;
use crate::import::folder_scanner::{CategorizedFiles, FileVersion, ScanItem};
use crate::import::list::{ImportListItem, ListedRelease};
use crate::import::triage::{
    import_status_of, MatchedRelease, TriageGroup, TriageMetadataSummary, TriageReading, TriageRow,
};
use crate::import::{CoverSelection, FolderReleaseDecisionKey, PressingLink, ReleaseLink};
use std::path::PathBuf;

/// The items `keys` name, in their order, loaded whole for the list `list`.
pub(super) fn materialise(
    sql: &SqlReadContext<'_>,
    list: ImportListId,
    keys: &[ItemKey],
) -> Result<Vec<WindowItemRows>, DbError> {
    let mut headers = header_rows(sql, list, keys.iter().filter(|key| key.item == Item::Group))?;
    let mut releases = release_rows(sql, list, keys.iter().filter(|key| key.item != Item::Group))?;
    let current: Vec<&str> = releases
        .values()
        .filter(|release| release.current)
        .filter_map(|release| release.content_hash.as_deref())
        .collect();
    let current = CandidateRows::Of(&current);
    let mut matches = load_matches_on(sql, current)?;
    let mut provenances = load_provenance_on(sql, current)?;
    let mut links = load_release_links_on(sql, current)?;
    let mut covers = load_covers_on(sql, current)?;
    keys.iter()
        .map(|key| {
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
            // The lookup's lead, the draft's source, the link and the cover
            // are the candidate's own only while its state describes its
            // current files.
            let hash = release.content_hash.clone().filter(|_| release.current);
            let hash = hash.as_ref();
            let lead = hash
                .and_then(|hash| matches.remove(hash))
                .and_then(|matches| matches.found.into_iter().next())
                .map(|stored| LeadMatch::of(&stored.result, Some(&stored.provenance)));
            let provenance = hash.and_then(|hash| provenances.remove(hash));
            let link = hash.and_then(|hash| links.remove(hash));
            let cover = hash.and_then(|hash| covers.remove(hash));
            candidate_item(sql, release, lead, provenance, link, cover)
        })
        .collect()
}

/// One group header's columns, read for the window that holds it.
fn header_rows<'a>(
    sql: &SqlReadContext<'_>,
    list: ImportListId,
    keys: impl Iterator<Item = &'a ItemKey>,
) -> Result<HashMap<(String, String), ImportListItem>, DbError> {
    let heads: Vec<serde_json::Value> = keys
        .map(|key| serde_json::json!([key.watched_folder_path, key.key]))
        .collect();
    if heads.is_empty() {
        return Ok(HashMap::new());
    }
    // A folder is offered to be read as one release when a stored reading of
    // it keeps its releases apart, or, with nothing stored, when it is the
    // nearest folder above some release holding two releases or more: no
    // folder between the two has nothing stored and two releases below it.
    let rows = sql.query(
        "WITH RECURSIVE \
             heads (root, name) AS ( \
                 SELECT value ->> 0, value ->> 1 FROM json_each(:heads)), \
             below (root, name, display_path) AS ( \
                 SELECT h.root, h.name, p.display_path FROM heads AS h \
                 JOIN import_list_placement AS p \
                     ON p.watched_folder_path = h.root \
                    AND p.display_path > h.name || '/' AND p.display_path < h.name || '0'), \
             inside (root, name, folder, rest) AS ( \
                 SELECT root, name, name, substr(display_path, length(name) + 2) FROM below \
                 UNION \
                 SELECT root, name, folder || '/' || substr(rest, 1, instr(rest, '/') - 1), \
                        substr(rest, instr(rest, '/') + 1) \
                 FROM inside WHERE instr(rest, '/') > 0), \
             nearer (root, name, folder) AS ( \
                 SELECT DISTINCT b.root, b.name, b.folder FROM inside AS b \
                 WHERE b.folder <> b.name \
                   AND (SELECT count(*) FROM ( \
                            SELECT 1 FROM import_list_placement AS q \
                            WHERE q.watched_folder_path = b.root \
                              AND q.display_path > b.folder || '/' \
                              AND q.display_path < b.folder || '0' LIMIT 2)) = 2 \
                   AND NOT EXISTS ( \
                       SELECT 1 FROM release_grouping AS g \
                       WHERE g.anchor_folder = \
                           rtrim(b.root, :sep) || :sep || replace(b.folder, '/', :sep))) \
         SELECT h.root, h.name, \
                NOT EXISTS (SELECT 1 FROM import_list_collapsed AS c \
                            WHERE c.list_id = :list AND c.watched_folder_path = h.root \
                              AND c.group_name = h.name) AS expanded, \
                (SELECT count(*) FROM import_list_matching AS m \
                 WHERE m.list_id = :list AND m.watched_folder_path = h.root \
                   AND m.group_name = h.name) AS entry_count, \
                COALESCE( \
                    (SELECT g.combined = 0 FROM release_grouping AS g \
                     WHERE g.anchor_folder = \
                         rtrim(h.root, :sep) || :sep || replace(h.name, '/', :sep)), \
                    (SELECT count(*) FROM (SELECT 1 FROM below AS b \
                                           WHERE b.root = h.root AND b.name = h.name \
                                           LIMIT 2)) = 2 \
                    AND EXISTS ( \
                        SELECT 1 FROM below AS b \
                        WHERE b.root = h.root AND b.name = h.name \
                          AND NOT EXISTS ( \
                              SELECT 1 FROM nearer AS n \
                              WHERE n.root = b.root AND n.name = b.name \
                                AND b.display_path > n.folder || '/' \
                                AND b.display_path < n.folder || '0'))) AS combinable \
         FROM heads AS h",
        named_params! {
            ":heads": serde_json::Value::Array(heads).to_string(),
            ":list": list.0,
            ":sep": std::path::MAIN_SEPARATOR_STR,
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
    in_group: bool,
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
    /// Whether the stored lookup settled on exactly one pressing.
    settled: bool,
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
            "SELECT p.watched_folder_path, p.key, p.display_path, p.group_name IS NOT NULL, \
                    p.has_summary, p.shown_title, p.shown_artists, {LISTED_COLUMNS}, \
                    c.name, c.content_hash, s.content_hash IS NOT NULL, \
                    origin.release_id, released.album_id, \
                    g.blocked, g.blocked_subject, g.blocked_holder, \
                    failure.content_hash IS NOT NULL AS failed, {FAILURE_REASON_COLUMNS}, \
                    {RUNTIME_FACT_COLUMNS}, \
                    v.kind IS 'found' AND (SELECT COUNT(DISTINCT m.pressing) \
                                           FROM import_candidate_match AS m \
                                           WHERE m.content_hash = v.content_hash \
                                             AND m.narrowed_out = 0) = 1 AS settled, \
                    EXISTS (SELECT 1 FROM import_list_selection AS chosen \
                            WHERE chosen.list_id = :list AND chosen.key = p.key) AS selected \
             FROM json_each(:keys) AS w \
             JOIN import_list_placement AS p ON p.key = w.value AND p.item <> 'group' \
             JOIN scan_candidate AS c \
                 ON c.watched_folder_path = p.watched_folder_path AND c.path = p.key \
             LEFT JOIN release_grouping AS g ON g.key = c.grouping_key \
             LEFT JOIN import_candidate_state AS s \
                 ON s.content_hash = c.content_hash AND s.edit_revision = c.file_edit_revision \
             LEFT JOIN import_candidate_verdict AS v ON v.content_hash = s.content_hash \
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
            let summary = match row.get::<_, Option<bool>>(4)? {
                Some(true) => Some(TriageMetadataSummary {
                    album_title: row.get(5)?,
                    album_artists: row.get(6)?,
                }),
                Some(false) | None => None,
            };
            let failed: bool = row.get("failed")?;
            Ok(ReleaseRows {
                watched_folder_path: row.get(0)?,
                key: row.get(1)?,
                display_path: row.get(2)?,
                in_group: row.get(3)?,
                summary,
                listed: ListedColumns::read(row, 7)?,
                name: row.get(18)?,
                content_hash: row.get(19)?,
                current: row.get(20)?,
                imported: match row.get::<_, Option<String>>(21)? {
                    Some(release_id) => Some(ImportedRelease {
                        release_id,
                        album_id: row.get(22)?,
                    }),
                    None => None,
                },
                block: (row.get(23)?, row.get(24)?, row.get(25)?),
                failure: match failed {
                    true => Some(failure_reason_from_row(row)?),
                    false => None,
                },
                facts: RuntimeFactColumns::read(row, 28)?,
                settled: row.get("settled")?,
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

/// A release's row: its placement and the keyed rows beside it.
fn candidate_item(
    sql: &SqlReadContext<'_>,
    release: ReleaseRows,
    lead: Option<LeadMatch>,
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
    // A linked release outranks the lookup's lead. A pressing's releases are
    // read in this snapshot and interpreted after it ends; an album names its
    // records itself.
    let picked = match release_link.as_ref() {
        Some(ReleaseLink::Pressing(pressing)) => {
            let files = load_candidate_on(sql, &release.key)?
                .ok_or_else(|| {
                    DbError::Message(format!(
                        "candidate {} vanished while reading its presentation",
                        release.key
                    ))
                })?
                .candidate
                .files;
            Some(LinkedRows::Pressing(picked_release(sql, pressing, files)?))
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
        // A draft read from somewhere with no link names no release, so
        // nothing leads the row then: the lookup's lead does not stand in
        // for a pick.
        matched: match (&release_link, &metadata_provenance) {
            (None, None) => lead.map(|lead| MatchedRelease::of_lead(&lead, release.settled)),
            (Some(_), _) | (None, Some(_)) => None,
        },
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
        is_group_member: release.in_group,
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
        is_group_member: bool,
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
                is_group_member,
            } => {
                match picked {
                    Some(LinkedRows::Pressing(picked)) => {
                        let PickedRelease { matched, records } = picked.process()?;
                        row.matched = Some(matched);
                        // The reading the row was placed with named no
                        // records; this is where the documents are read.
                        row.reading = TriageReading::of(
                            row.metadata_summary.as_ref(),
                            row.release_link.as_ref(),
                            records,
                        );
                    }
                    // An album is no pressing, so nothing leads the row: the
                    // lookup's lead is one of the pressings the person could
                    // not tell their copy from.
                    Some(LinkedRows::Album(records)) => {
                        row.reading = TriageReading::of(
                            row.metadata_summary.as_ref(),
                            row.release_link.as_ref(),
                            records,
                        );
                    }
                    None => {}
                }
                row.cover = cover;
                Ok(ImportListItem::Candidate {
                    row,
                    live,
                    is_group_member,
                })
            }
        }
    }
}

/// What a linked row's link reads: a pressing's stored releases, or an
/// album's records, which the link names itself.
#[derive(Clone, PartialEq)]
pub(super) enum LinkedRows {
    Pressing(PickedReleaseRows),
    Album(Vec<crate::import::ReleaseRecord>),
}

#[derive(Clone, PartialEq)]
pub(super) struct PickedReleaseRows {
    /// Every release the link claims, the primary first and then its
    /// partners.
    claimed: Vec<crate::import::source_release::SourceRelease>,
    files: CategorizedFiles,
}

/// What the picked releases say: the release the row leads with, and every
/// catalog that describes it.
pub(super) struct PickedRelease {
    matched: MatchedRelease,
    records: Vec<crate::import::ReleaseRecord>,
}

impl PickedReleaseRows {
    fn process(self) -> Result<PickedRelease, DbError> {
        // The folder's audio as it stands — its rip logs and sheets read
        // now — which the primary's tracklist is counted against.
        let audio = crate::import::catalog_reading::ReadAudio::of_files(&self.files)
            .map_err(|error| DbError::Message(error.to_string()))?;
        let records = crate::import::source_release::claimed_records(
            &self.claimed.iter().collect::<Vec<_>>(),
        );
        // Only the link's primary release states the row's facts; a
        // partner's own release is not a second set of them. Its artwork is
        // another matter: a row's cover is the pick's, so the partners go to
        // the detail that carries the cover options.
        let (primary, partners) = self
            .claimed
            .split_first()
            .expect("a pick claims at least its primary");
        let detail = primary.detail(partners);
        Ok(PickedRelease {
            matched: MatchedRelease::of_pick(
                primary.release().catalog,
                &detail,
                crate::import::catalog_reading::listing(&primary.read(&audio), &audio),
            ),
            records,
        })
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
    let snapshot = super::super::folder_scans::load_candidate_file_tag_snapshot(
        sql,
        watched_folder_path,
        candidate_path,
    )?
    .and_then(|stored| stored.snapshot)
    .ok_or_else(|| {
        DbError::Message(format!(
            "candidate {candidate_path} selects embedded cover without a file-tag snapshot"
        ))
    })?;
    let cover = snapshot.embedded_cover.ok_or_else(|| {
        DbError::Message(format!(
            "candidate {candidate_path} selects embedded cover without stored artwork"
        ))
    })?;
    if cover.source_relative_path != source_file_id {
        return Err(DbError::Message(format!(
            "candidate {candidate_path} selects embedded cover from {source_file_id}, but its snapshot stores {}",
            cover.source_relative_path
        )));
    }
    let observation = snapshot
        .files
        .iter()
        .map(|fact| &fact.observation)
        .find(|observation| observation.relative_path == source_file_id)
        .ok_or_else(|| {
            DbError::Message(format!(
                "candidate {candidate_path}'s snapshot stores a cover from {source_file_id} but no reading of that file"
            ))
        })?;
    let scanned = scanned_file_version(sql, watched_folder_path, candidate_path, source_file_id)?;
    Ok((
        FileVersion {
            path: scanned.path,
            size: observation.size,
            modified_at_ns: observation.modified_at_ns,
        },
        cover.data,
    ))
}

/// Every release a linked pressing claims, with the stored release for each.
fn picked_release(
    sql: &SqlReadContext<'_>,
    link: &PressingLink,
    files: CategorizedFiles,
) -> Result<PickedReleaseRows, DbError> {
    let claimed = link
        .claimed()
        .map(|release| {
            load_source_release_on(sql, release)?.ok_or_else(|| {
                DbError::Message(format!(
                    "{} release {} is linked but nothing stored it",
                    release.catalog.as_str(),
                    release.key
                ))
            })
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    Ok(PickedReleaseRows { claimed, files })
}
