//! Reading a candidate back: its row, its verdict with the matches that hang
//! off it, its draft's provenance, its release link, and its file decisions.

use super::super::query::CandidateRows;
use super::edit_rows::{read_file_edits, FILE_EDITS_COLUMNS};
use super::lookup_choice_rows::LOOKUP_CHOICES_COLUMNS;
use super::verdict_rows::{
    identification_of, match_of, read_match_row, read_verdict_row, unreadable, MatchColumns,
    MatchEntries, StoredMatches, StoredMedium, VerdictRow, VERDICT_COLUMNS,
};
use super::*;
use crate::import::{AlbumLink, MetadataProvenance, MetadataRef, PressingLink, ReleaseLink};
use bae_catalogs::catalog::Catalog;
use std::str::FromStr;

/// The provenance as its columns. Only an external release names a source and
/// a release.
struct ProvenanceColumns<'a> {
    kind: &'static str,
    source: Option<&'static str>,
    release_id: Option<&'a str>,
}

fn provenance_columns(provenance: &MetadataProvenance) -> ProvenanceColumns<'_> {
    match provenance {
        MetadataProvenance::FileMetadata => ProvenanceColumns {
            kind: "file_tags",
            source: None,
            release_id: None,
        },
        MetadataProvenance::ExternalRelease { record, .. } => ProvenanceColumns {
            kind: "external_release",
            source: Some(record.catalog.as_str()),
            release_id: Some(record.key.as_str()),
        },
    }
}

/// Write the draft's provenance.
///
/// Called after the draft row is replaced, which cascades the previous
/// provenance away, so this only ever inserts.
pub(super) fn insert_provenance(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    provenance: &MetadataProvenance,
) -> Result<(), DbError> {
    let columns = provenance_columns(provenance);
    sql.execute(
        "INSERT INTO import_candidate_draft_provenance \
             (content_hash, kind, source, release_id) \
         VALUES (?, ?, ?, ?)",
        params![
            content_hash,
            columns.kind,
            columns.source,
            columns.release_id
        ],
    )?;
    Ok(())
}

/// A candidate's draft provenance as columns of a read that joins its row as
/// `provenance`. A candidate with none reads `provenance_row` as NULL.
pub(crate) const PROVENANCE_COLUMNS: &str = "provenance.content_hash AS provenance_row, \
     provenance.kind AS provenance_kind, provenance.source AS provenance_source, \
     provenance.release_id AS provenance_release_id";

/// One candidate's [`PROVENANCE_COLUMNS`] as SQLite hands them over.
pub(crate) struct StoredProvenance {
    kind: String,
    source: Option<String>,
    release_id: Option<String>,
}

/// The [`PROVENANCE_COLUMNS`] of `row`; none for a draft that records no
/// provenance.
pub(crate) fn read_provenance(row: &Row<'_>) -> rusqlite::Result<Option<StoredProvenance>> {
    if row.get::<_, Option<String>>("provenance_row")?.is_none() {
        return Ok(None);
    }
    Ok(Some(StoredProvenance {
        kind: row.get("provenance_kind")?,
        source: row.get("provenance_source")?,
        release_id: row.get("provenance_release_id")?,
    }))
}

impl StoredProvenance {
    pub(crate) fn provenance(self) -> Result<MetadataProvenance, DbError> {
        match self.kind.as_str() {
            "file_tags" => Ok(MetadataProvenance::FileMetadata),
            "external_release" => {
                let missing = |what: &str| {
                    DbError::Message(format!(
                        "stored external metadata provenance names no {what}"
                    ))
                };
                Ok(MetadataProvenance::ExternalRelease {
                    record: MetadataRef::new(
                        Catalog::from_str(&self.source.ok_or_else(|| missing("catalog"))?)
                            .map_err(DbError::Message)?,
                        self.release_id.ok_or_else(|| missing("release"))?,
                    ),
                })
            }
            other => Err(unreadable("provenance kind", other)),
        }
    }
}

/// The draft provenance of the candidate `content_hash`, if its draft
/// records one; on whichever connection the caller holds.
pub(super) fn load_provenance_on(
    sql: &(impl super::super::query::QueryOne + super::super::query::QueryRows),
    content_hash: &str,
) -> Result<Option<MetadataProvenance>, DbError> {
    sql.query_row(
        &format!(
            "SELECT {PROVENANCE_COLUMNS} FROM import_candidate_draft_provenance AS provenance \
             WHERE provenance.content_hash = ?"
        ),
        [content_hash],
        read_provenance,
    )
    .optional()?
    .flatten()
    .map(StoredProvenance::provenance)
    .transpose()
}

/// Replace the candidate's release link with `link`, or remove it.
pub(super) fn replace_release_link(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    link: Option<&ReleaseLink>,
) -> Result<(), DbError> {
    // The partner and album rows hang off the link row, so this clears them
    // too.
    sql.execute(
        "DELETE FROM import_candidate_release_link WHERE content_hash = ?",
        [content_hash],
    )?;
    match link {
        None => {}
        Some(ReleaseLink::Pressing(pressing)) => {
            sql.execute(
                "INSERT INTO import_candidate_release_link \
                     (content_hash, kind, source, release_id) \
                 VALUES (?, 'pressing', ?, ?)",
                params![
                    content_hash,
                    pressing.record.catalog.as_str(),
                    pressing.record.key
                ],
            )?;
            for partner in &pressing.partners {
                sql.execute(
                    "INSERT INTO import_candidate_release_link_partner \
                         (content_hash, kind, source, release_id) \
                     VALUES (?, 'pressing', ?, ?)",
                    params![content_hash, partner.catalog.as_str(), partner.key],
                )?;
            }
        }
        Some(ReleaseLink::Album(album)) => {
            sql.execute(
                "INSERT INTO import_candidate_release_link (content_hash, kind) \
                 VALUES (?, 'album')",
                [content_hash],
            )?;
            for album in album.albums() {
                sql.execute(
                    "INSERT INTO import_candidate_release_link_album \
                         (content_hash, kind, catalog, album_id) \
                     VALUES (?, 'album', ?, ?)",
                    params![content_hash, album.catalog.as_str(), album.key],
                )?;
            }
        }
    }
    Ok(())
}

/// A candidate's release link as columns of a read that joins its row as
/// `link`: a pressing's partners in the other catalogs, by catalog, and an
/// album's album in each catalog, in catalog order, riding on it as JSON
/// arrays. A candidate with no link reads `link_row` as NULL.
pub(crate) const RELEASE_LINK_COLUMNS: &str = "link.content_hash AS link_row, \
     link.kind AS link_kind, link.source AS link_source, link.release_id AS link_release_id, \
     (SELECT json_group_array(json_array(partner.source, partner.release_id) \
                              ORDER BY partner.source) \
      FROM import_candidate_release_link_partner AS partner \
      WHERE partner.content_hash = link.content_hash) AS link_partners, \
     (SELECT json_group_array(json_array(album.catalog, album.album_id) \
                              ORDER BY bae_catalog_rank(album.catalog)) \
      FROM import_candidate_release_link_album AS album \
      WHERE album.content_hash = link.content_hash) AS link_albums";

/// One candidate's [`RELEASE_LINK_COLUMNS`] as SQLite hands them over, its
/// lists still JSON.
pub(crate) struct StoredReleaseLink {
    content_hash: String,
    kind: String,
    source: Option<String>,
    release_id: Option<String>,
    partners: String,
    albums: String,
}

/// The [`RELEASE_LINK_COLUMNS`] of `row`; none for an unlinked candidate.
pub(crate) fn read_release_link(row: &Row<'_>) -> rusqlite::Result<Option<StoredReleaseLink>> {
    let Some(content_hash) = row.get::<_, Option<String>>("link_row")? else {
        return Ok(None);
    };
    Ok(Some(StoredReleaseLink {
        content_hash,
        kind: row.get("link_kind")?,
        source: row.get("link_source")?,
        release_id: row.get("link_release_id")?,
        partners: row.get("link_partners")?,
        albums: row.get("link_albums")?,
    }))
}

impl StoredReleaseLink {
    pub(crate) fn link(self) -> Result<ReleaseLink, DbError> {
        let Self {
            content_hash,
            kind,
            source,
            release_id,
            partners,
            albums,
        } = self;
        let references = |json: &str, what: &str| {
            serde_json::from_str::<Vec<(String, String)>>(json)
                .map_err(|error| {
                    DbError::Message(format!("{content_hash}'s stored {what} {json:?}: {error}"))
                })?
                .into_iter()
                .map(|(catalog, key)| reference(catalog, key))
                .collect::<Result<Vec<_>, DbError>>()
        };
        match (kind.as_str(), source, release_id) {
            ("pressing", Some(source), Some(release_id)) => {
                Ok(ReleaseLink::Pressing(PressingLink {
                    record: reference(source, release_id)?,
                    partners: references(&partners, "link partners")?,
                }))
            }
            ("album", None, None) => Ok(ReleaseLink::Album(
                AlbumLink::new(references(&albums, "linked albums")?).ok_or_else(|| {
                    DbError::Message(format!(
                        "{content_hash} is linked to an album no catalog names"
                    ))
                })?,
            )),
            (other, _, _) => Err(unreadable("release link", other)),
        }
    }
}

fn reference(catalog: String, key: String) -> Result<MetadataRef, DbError> {
    Ok(MetadataRef::new(
        Catalog::from_str(&catalog).map_err(DbError::Message)?,
        key,
    ))
}

/// The release link of the candidate `content_hash`, with its partners or
/// its albums, if it is linked; on whichever connection the caller holds.
pub(super) fn load_release_link_on(
    sql: &(impl super::super::query::QueryOne + super::super::query::QueryRows),
    content_hash: &str,
) -> Result<Option<ReleaseLink>, DbError> {
    sql.query_row(
        &format!(
            "SELECT {RELEASE_LINK_COLUMNS} FROM import_candidate_release_link AS link \
             WHERE link.content_hash = ?"
        ),
        [content_hash],
        read_release_link,
    )
    .optional()?
    .flatten()
    .map(StoredReleaseLink::link)
    .transpose()
}

const MATCH_COLUMNS: &str = "found.position, found.pressing, found.source, found.release_id, \
     found.title, found.artist, found.year, found.labels, found.country, found.region, \
     found.status, found.packaging, found.discogs_details, found.media_kind, found.cover_url, \
     found.cover_label, found.cover_source, found.cover_standing, found.source_group_id, \
     found.album_links, found.source_tracks_kind, found.source_tracks_count, \
     found.by_disc_id, found.by_barcode, found.by_catalog, found.by_isrc, found.by_search, \
     found.by_pressing, found.narrowed_out, found.document_failure, \
     found.document_failure_status, found.album_first_year, found.track_titles, found.notes, \
     found.named_note";

/// What hangs off a match joined as `found`, each list a JSON array in its
/// stored order: its barcodes, its cover's copies, its media, its links and
/// its album links.
fn match_entries_columns() -> String {
    let child = |table: &str, value: &str, order: &str| {
        format!(
            "(SELECT json_group_array({value} ORDER BY {order}) FROM {table} AS entry \
              WHERE entry.content_hash = found.content_hash AND entry.position = found.position)"
        )
    };
    format!(
        "{barcodes} AS match_barcodes, {copies} AS match_cover_copies, \
         {media} AS match_media, {links} AS match_links, {album_links} AS match_album_links",
        barcodes = child(
            "import_candidate_match_barcode",
            "entry.barcode",
            "entry.ordinal"
        ),
        copies = super::super::downscaled_copies::copies_json_sql(
            "import_candidate_match_cover_copy",
            "copy.content_hash = found.content_hash AND copy.position = found.position",
        ),
        media = child(
            "import_candidate_match_medium",
            "json_object('kind', entry.media_kind, 'medium', entry.medium, \
                         'quantity', entry.quantity)",
            "entry.ordinal",
        ),
        links = child(
            "import_candidate_match_link",
            "json_array(entry.catalog, entry.key)",
            "entry.ordinal",
        ),
        album_links = child(
            "import_candidate_match_album_link",
            "json_object('catalog', entry.catalog, 'key', entry.key, 'stated', entry.stated, \
                         'wikidata_item', entry.wikidata_item, \
                         'musicbrainz_release', entry.musicbrainz_release, \
                         'release_catalog', entry.release_catalog, \
                         'release_key', entry.release_key)",
            "entry.ordinal",
        ),
    )
}

/// One match's child lists as SQLite hands them over, still JSON.
struct StoredMatchEntries {
    barcodes: String,
    cover_copies: String,
    media: String,
    links: String,
    album_links: String,
}

fn read_match_entries(row: &Row<'_>) -> rusqlite::Result<StoredMatchEntries> {
    Ok(StoredMatchEntries {
        barcodes: row.get("match_barcodes")?,
        cover_copies: row.get("match_cover_copies")?,
        media: row.get("match_media")?,
        links: row.get("match_links")?,
        album_links: row.get("match_album_links")?,
    })
}

/// One medium of a match as [`match_entries_columns`] holds it.
#[derive(serde::Deserialize)]
struct StoredMatchMedium {
    kind: String,
    medium: Option<String>,
    quantity: u32,
}

impl StoredMatchEntries {
    fn entries(self) -> Result<MatchEntries, DbError> {
        fn list<T: serde::de::DeserializeOwned>(json: &str, what: &str) -> Result<Vec<T>, DbError> {
            serde_json::from_str(json).map_err(|error| {
                DbError::Message(format!("a match's stored {what} {json:?}: {error}"))
            })
        }
        Ok(MatchEntries {
            barcodes: list(&self.barcodes, "barcodes")?,
            cover_copies: super::super::downscaled_copies::copies_of(&self.cover_copies)?,
            media: list::<StoredMatchMedium>(&self.media, "media")?
                .into_iter()
                .map(|stored| {
                    Ok(StoredMedium {
                        kind: stored.kind,
                        medium: stored
                            .medium
                            .map(|key| {
                                bae_types::pressing::Medium::from_key(&key)
                                    .ok_or_else(|| unreadable("medium", &key))
                            })
                            .transpose()?,
                        quantity: stored.quantity,
                    })
                })
                .collect::<Result<_, DbError>>()?,
            links: list(&self.links, "links")?,
            album_links: list(&self.album_links, "album links")?,
        })
    }
}

/// One candidate's own columns and the rows joined one-to-one onto it, as
/// SQLite hands them over: everything of the candidate but its matches.
struct StoredCandidate {
    content_hash: String,
    folder_path: String,
    metadata_revision: i64,
    author: Option<String>,
    file_edits: super::edit_rows::StoredFileEdits,
    signals: Option<super::signal_rows::StoredSignals>,
    provenance: Option<StoredProvenance>,
    link: Option<StoredReleaseLink>,
    lookup_choices: Option<super::lookup_choice_rows::StoredLookupChoices>,
    verdict: Option<VerdictRow>,
    matches: StoredMatches,
}

/// One row of [`load_states_rows_on`]'s read: the candidate's columns on the
/// first row of each candidate, and the match the row joins, if any.
struct StateReadRow {
    candidate: Option<Result<StoredCandidate, DbError>>,
    found: Option<Result<(MatchColumns, StoredMatchEntries), DbError>>,
}

/// The candidate `content_hash` as the first of its rows reads it.
fn read_candidate(row: &Row<'_>, content_hash: String) -> Result<StoredCandidate, DbError> {
    Ok(StoredCandidate {
        content_hash,
        folder_path: row.get("folder_path")?,
        metadata_revision: row.get("metadata_revision")?,
        author: row.get("draft_author")?,
        file_edits: read_file_edits(row)?,
        signals: super::signal_rows::read_signals(row)?,
        provenance: read_provenance(row)?,
        link: read_release_link(row)?,
        lookup_choices: super::lookup_choice_rows::read_lookup_choices(row)?,
        verdict: read_verdict_row(row)?,
        matches: StoredMatches::default(),
    })
}

/// The match a row joins, if it joins one.
fn read_found(row: &Row<'_>) -> Result<Option<(MatchColumns, StoredMatchEntries)>, DbError> {
    if row.get::<_, Option<i64>>("position")?.is_none() {
        return Ok(None);
    }
    Ok(Some((read_match_row(row)?, read_match_entries(row)?)))
}

/// Every stored candidate row, or the one `only` names, assembled with the
/// verdict, signals, provenance, and file-decision rows that hang off it.
pub(crate) fn load_states_on(
    sql: &SqlReadContext<'_>,
    only: CandidateRows<'_>,
) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
    load_states_rows_on(sql, only)?()
}

/// The candidates `only` names in one read: each state row with the rows
/// that hang off it one-to-one joined onto it once, then joined to its
/// matches in position order, one row per match.
pub(crate) fn load_states_rows_on(
    sql: &SqlReadContext<'_>,
    only: CandidateRows<'_>,
) -> Result<
    impl FnOnce() -> Result<HashMap<String, DbImportCandidateState>, DbError> + Send + 'static,
    DbError,
> {
    let filter = only.filter("state.content_hash");
    let mut previous: Option<String> = None;
    let rows = sql.query(
        &format!(
            "WITH candidate AS MATERIALIZED ( \
                 SELECT state.content_hash, state.folder_path, state.metadata_revision, \
                        edit.author AS draft_author, {FILE_EDITS_COLUMNS}, {signals}, \
                        {PROVENANCE_COLUMNS}, {RELEASE_LINK_COLUMNS}, {LOOKUP_CHOICES_COLUMNS}, \
                        {VERDICT_COLUMNS} \
                 FROM import_candidate_state AS state \
                 LEFT JOIN import_candidate_edit AS edit \
                     ON edit.content_hash = state.content_hash \
                 LEFT JOIN import_candidate_signals AS signals \
                     ON signals.content_hash = state.content_hash \
                 LEFT JOIN import_candidate_draft_provenance AS provenance \
                     ON provenance.content_hash = state.content_hash \
                 LEFT JOIN import_candidate_release_link AS link \
                     ON link.content_hash = state.content_hash \
                 LEFT JOIN import_candidate_lookup_choices AS choices \
                     ON choices.content_hash = state.content_hash \
                 LEFT JOIN import_candidate_verdict AS verdict \
                     ON verdict.content_hash = state.content_hash \
                 {clause}) \
             SELECT candidate.*, {MATCH_COLUMNS}, {entries} \
             FROM candidate \
             LEFT JOIN import_candidate_match AS found \
                 ON found.content_hash = candidate.content_hash \
             ORDER BY candidate.content_hash, found.position",
            signals = super::signal_rows::signals_read_columns(),
            entries = match_entries_columns(),
            clause = filter.clause,
        ),
        filter.params().as_slice(),
        |row| {
            let content_hash: String = row.get("content_hash")?;
            let candidate = if previous.as_deref() == Some(content_hash.as_str()) {
                None
            } else {
                previous = Some(content_hash.clone());
                Some(read_candidate(row, content_hash))
            };
            Ok(StateReadRow {
                candidate,
                found: read_found(row).transpose(),
            })
        },
    )?;
    Ok(move || {
        let mut candidates: Vec<StoredCandidate> = Vec::new();
        for row in rows {
            if let Some(candidate) = row.candidate {
                candidates.push(candidate?);
            }
            let Some(found) = row.found else {
                continue;
            };
            let (columns, entries) = found?;
            let candidate = candidates
                .last_mut()
                .expect("a match row follows its candidate's first row");
            let found = match_of(columns, entries.entries()?)?;
            let list = if found.narrowed_out {
                &mut candidate.matches.narrowed_out
            } else {
                &mut candidate.matches.found
            };
            list.push(found.stored);
        }
        candidates
            .into_iter()
            .map(|candidate| Ok((candidate.content_hash.clone(), candidate.state()?)))
            .collect()
    })
}

impl StoredCandidate {
    fn state(self) -> Result<DbImportCandidateState, DbError> {
        let Self {
            content_hash,
            folder_path,
            metadata_revision,
            author,
            file_edits,
            signals,
            provenance,
            link,
            lookup_choices,
            verdict,
            matches,
        } = self;
        let metadata_revision = u64::try_from(metadata_revision).map_err(|error| {
            DbError::Message(format!(
                "import candidate {content_hash} has a negative metadata revision: {error}"
            ))
        })?;
        let author = author.ok_or_else(|| {
            DbError::Message(format!(
                "candidate {content_hash} has no editable metadata draft"
            ))
        })?;
        let author = super::pane_rows::author_of(&author)?;
        let provenance = provenance.map(StoredProvenance::provenance).transpose()?;
        author
            .check_provenance(provenance.as_ref())
            .map_err(|error| DbError::Message(format!("candidate {content_hash}: {error}")))?;
        let identify = match verdict {
            Some(verdict) => Some(identification_of(verdict, matches)?),
            // The match rows hang off the verdict row, so a candidate
            // without one has none.
            None if matches.found.is_empty() && matches.narrowed_out.is_empty() => None,
            None => {
                return Err(DbError::Message(format!(
                    "candidate {content_hash} holds matches but no verdict"
                )))
            }
        };
        Ok(DbImportCandidateState {
            signals: signals.map(|stored| stored.signals()).transpose()?,
            lookup_choices: lookup_choices
                .map(|stored| stored.choices())
                .transpose()?
                .unwrap_or_default(),
            identify,
            metadata_author: author,
            metadata_provenance: provenance,
            release_link: link.map(StoredReleaseLink::link).transpose()?,
            file_edits: file_edits.edits()?,
            content_hash,
            folder_path,
            metadata_revision,
        })
    }
}

/// The file decisions a read fetched, still to be assembled — boxed so a read
/// on either connection hands back one type.
type FinishEdits<T> = Box<dyn FnOnce() -> Result<T, DbError> + Send + 'static>;

/// The file decisions of every candidate a decision was made for, each at its
/// revision: the candidates whose decisions stand, and those whose revision
/// moved past zero though none stands now. Any other candidate has none at
/// revision zero, which is what a scan reads where nothing is stored.
pub(crate) fn load_stored_candidate_edits_on(
    sql: &(impl super::super::query::QueryOne + super::super::query::QueryRows),
) -> Result<FinishEdits<crate::import::folder_scanner::StoredCandidateEdits>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT state.content_hash, {FILE_EDITS_COLUMNS} \
             FROM import_candidate_state AS state \
             WHERE state.edit_revision > 0 \
                OR EXISTS (SELECT 1 FROM import_candidate_sheet_disc AS decided \
                           WHERE decided.content_hash = state.content_hash) \
                OR EXISTS (SELECT 1 FROM import_candidate_sheet_reference AS decided \
                           WHERE decided.content_hash = state.content_hash)"
        ),
        [],
        read_file_edits,
    )?;
    Ok(Box::new(move || {
        let edits = rows
            .into_iter()
            .map(|stored| Ok((stored.content_hash().to_string(), stored.edits()?)))
            .collect::<Result<HashMap<_, _>, DbError>>()?;
        Ok(crate::import::folder_scanner::StoredCandidateEdits::new(
            edits,
        ))
    }))
}

/// One candidate's file decisions, revision included; none at revision zero
/// for a candidate with no state row. Progressive scans call this after they
/// compute a content hash, so each emitted row performs one indexed lookup
/// instead of rereading the whole table.
pub(crate) fn load_candidate_file_edits_on(
    sql: &(impl super::super::query::QueryOne + super::super::query::QueryRows),
    content_hash: &str,
) -> Result<FinishEdits<CandidateFileEdits>, DbError> {
    let stored = sql
        .query_row(
            &format!(
                "SELECT state.content_hash, {FILE_EDITS_COLUMNS} \
                 FROM import_candidate_state AS state WHERE state.content_hash = ?"
            ),
            [content_hash],
            read_file_edits,
        )
        .optional()?;
    Ok(Box::new(move || {
        stored.map_or_else(
            || Ok(CandidateFileEdits::default()),
            |stored| stored.edits(),
        )
    }))
}
