//! Reading a candidate back: its row, its verdict with the matches that hang
//! off it, its draft's provenance, its release link, and its file decisions.

use super::super::query::CandidateRows;
use super::edit_rows::{read_file_edits, FILE_EDITS_COLUMNS};
use super::lookup_choice_rows::load_lookup_choices_on;
use super::signal_rows::load_signals_on;
use super::verdict_rows::{
    identification_of, match_of, read_match_row, read_verdict_row, unreadable, MatchEntries,
    StoredMatches, StoredMedium, VERDICT_COLUMNS,
};
use super::*;
use crate::import::{AlbumLink, MetadataProvenance, MetadataRef, PressingLink, ReleaseLink};
use bae_catalogs::catalog::Catalog;
use std::str::FromStr;

type CandidateProvenances = HashMap<String, MetadataProvenance>;
type CandidateLinks = HashMap<String, ReleaseLink>;

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
pub(super) const PROVENANCE_COLUMNS: &str = "provenance.content_hash AS provenance_row, \
     provenance.kind AS provenance_kind, provenance.source AS provenance_source, \
     provenance.release_id AS provenance_release_id";

/// One candidate's [`PROVENANCE_COLUMNS`] as SQLite hands them over.
pub(super) struct StoredProvenance {
    kind: String,
    source: Option<String>,
    release_id: Option<String>,
}

/// The [`PROVENANCE_COLUMNS`] of `row`; none for a draft that records no
/// provenance.
pub(super) fn read_provenance(row: &Row<'_>) -> rusqlite::Result<Option<StoredProvenance>> {
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
    pub(super) fn provenance(self) -> Result<MetadataProvenance, DbError> {
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

/// The draft provenance of every candidate `only` names.
pub(crate) fn load_provenance_on(
    sql: &impl super::super::query::QueryRows,
    only: CandidateRows<'_>,
) -> Result<CandidateProvenances, DbError> {
    load_provenance_rows_on(sql, only)?()
}

pub(crate) fn load_provenance_rows_on(
    sql: &impl super::super::query::QueryRows,
    only: CandidateRows<'_>,
) -> Result<Box<dyn FnOnce() -> Result<CandidateProvenances, DbError> + Send + 'static>, DbError> {
    let only = only.filter("provenance.content_hash");
    let rows = sql.query(
        &format!(
            "SELECT {PROVENANCE_COLUMNS} \
             FROM import_candidate_draft_provenance AS provenance {}",
            only.clause
        ),
        only.params().as_slice(),
        |row| {
            Ok((
                row.get::<_, String>("provenance_row")?,
                read_provenance(row)?,
            ))
        },
    )?;
    Ok(Box::new(move || {
        rows.into_iter()
            .filter_map(|(content_hash, stored)| Some((content_hash, stored?)))
            .map(|(content_hash, stored)| Ok((content_hash, stored.provenance()?)))
            .collect()
    }))
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
pub(super) const RELEASE_LINK_COLUMNS: &str = "link.content_hash AS link_row, \
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
pub(super) struct StoredReleaseLink {
    content_hash: String,
    kind: String,
    source: Option<String>,
    release_id: Option<String>,
    partners: String,
    albums: String,
}

/// The [`RELEASE_LINK_COLUMNS`] of `row`; none for an unlinked candidate.
pub(super) fn read_release_link(row: &Row<'_>) -> rusqlite::Result<Option<StoredReleaseLink>> {
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
    pub(super) fn link(self) -> Result<ReleaseLink, DbError> {
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

/// The release link of every candidate `only` names, each with its partners
/// or its albums.
pub(crate) fn load_release_links_on(
    sql: &impl super::super::query::QueryRows,
    only: CandidateRows<'_>,
) -> Result<CandidateLinks, DbError> {
    load_release_link_rows_on(sql, only)?()
}

pub(crate) fn load_release_link_rows_on(
    sql: &impl super::super::query::QueryRows,
    only: CandidateRows<'_>,
) -> Result<Box<dyn FnOnce() -> Result<CandidateLinks, DbError> + Send + 'static>, DbError> {
    let only = only.filter("link.content_hash");
    let rows = sql.query(
        &format!(
            "SELECT {RELEASE_LINK_COLUMNS} FROM import_candidate_release_link AS link {}",
            only.clause
        ),
        only.params().as_slice(),
        read_release_link,
    )?;
    Ok(Box::new(move || {
        rows.into_iter()
            .flatten()
            .map(|stored| Ok((stored.content_hash.clone(), stored.link()?)))
            .collect()
    }))
}

/// Every candidate's verdict, or the one `only` names, each rebuilt with the
/// matches that hang off it.
pub(crate) fn load_verdicts_on(
    sql: &SqlReadContext<'_>,
    only: CandidateRows<'_>,
) -> Result<
    impl FnOnce() -> Result<HashMap<String, DbCandidateIdentifyResult>, DbError> + Send + 'static,
    DbError,
> {
    let filter = only.filter("content_hash");
    let clause = &filter.clause;
    let matches = load_matches_rows_on(sql, only)?;
    let rows = sql.query(
        &format!(
            "SELECT {VERDICT_COLUMNS} FROM import_candidate_verdict \
             {clause}"
        ),
        filter.params().as_slice(),
        |row| Ok(read_verdict_row(row)),
    )?;
    Ok(move || {
        let mut matches = matches()?;
        let mut out = HashMap::with_capacity(rows.len());
        for row in rows {
            let row = row?;
            let content_hash = row.content_hash.clone();
            let found = matches.remove(&content_hash).unwrap_or_default();
            out.insert(content_hash, identification_of(row, found)?);
        }
        Ok(out)
    })
}

struct StateRow {
    content_hash: String,
    folder_path: String,
    metadata_revision: i64,
}

fn read_state_row(row: &Row<'_>) -> Result<StateRow, DbError> {
    Ok(StateRow {
        content_hash: row.get("content_hash")?,
        folder_path: row.get("folder_path")?,
        metadata_revision: row.get("metadata_revision")?,
    })
}

const STATE_COLUMNS: &str = "content_hash, folder_path, metadata_revision";

const MATCH_COLUMNS: &str = "content_hash, position, pressing, source, release_id, title, artist, \
     year, labels, country, region, status, packaging, discogs_details, \
     media_kind, cover_url, \
     cover_label, cover_source, cover_standing, source_group_id, album_links, source_tracks_kind, \
     source_tracks_count, \
     by_disc_id, by_barcode, by_catalog, by_isrc, by_search, by_pressing, narrowed_out, \
     document_failure, document_failure_status, album_first_year, \
     track_titles, notes, named_note";

/// Every match a read fetched, still to be assembled.
type FinishMatches =
    Box<dyn FnOnce() -> Result<HashMap<String, StoredMatches>, DbError> + Send + 'static>;

/// The stored matches of every candidate `only` names, keyed by content
/// hash, read now and assembled by the closure it answers with.
///
/// The whole row per match rather than a count and a lead, and the barcode,
/// medium and link rows that hang off it: how many *pressings* a verdict named
/// is decided by grouping them, which needs everything each one states. The
/// one reader of these columns, for the pane's whole verdict.
pub(crate) fn load_matches_rows_on(
    sql: &(impl super::super::query::QueryOne + super::super::query::QueryRows),
    only: CandidateRows<'_>,
) -> Result<FinishMatches, DbError> {
    let only = only.filter("content_hash");
    let rows = sql.query(
        &format!(
            "SELECT {MATCH_COLUMNS} FROM import_candidate_match \
             {} ORDER BY content_hash, position",
            only.clause
        ),
        only.params().as_slice(),
        |row| Ok(read_match_row(row)),
    )?;
    let barcodes = sql.query(
        &format!(
            "SELECT content_hash, position, barcode FROM import_candidate_match_barcode \
             {} \
             ORDER BY content_hash, position, ordinal",
            only.clause
        ),
        only.params().as_slice(),
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )?;
    let cover_copies = sql.query(
        &format!(
            "SELECT content_hash, position, max_edge, url FROM import_candidate_match_cover_copy \
             {}",
            only.clause
        ),
        only.params().as_slice(),
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
    let media = sql.query(
        &format!(
            "SELECT content_hash, position, media_kind, medium, quantity \
             FROM import_candidate_match_medium \
             {} \
             ORDER BY content_hash, position, ordinal",
            only.clause
        ),
        only.params().as_slice(),
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                StoredMedium {
                    kind: row.get(2)?,
                    medium: super::super::pressing_columns::keyed(
                        row,
                        "medium",
                        bae_types::pressing::Medium::from_key,
                    )?,
                    quantity: row.get(4)?,
                },
            ))
        },
    )?;
    let links = sql.query(
        &format!(
            "SELECT content_hash, position, catalog, key FROM import_candidate_match_link \
             {} \
             ORDER BY content_hash, position, ordinal",
            only.clause
        ),
        only.params().as_slice(),
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
    let album_links = sql.query(
        &format!(
            "SELECT content_hash, position, catalog, key, stated, wikidata_item, \
                musicbrainz_release, release_catalog, release_key \
             FROM import_candidate_match_album_link \
             {} \
             ORDER BY content_hash, position, ordinal",
            only.clause
        ),
        only.params().as_slice(),
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                super::super::album_link_rows::AlbumLinkRow {
                    catalog: row.get(2)?,
                    key: row.get(3)?,
                    stated: row.get(4)?,
                    wikidata_item: row.get(5)?,
                    musicbrainz_release: row.get(6)?,
                    release_catalog: row.get(7)?,
                    release_key: row.get(8)?,
                },
            ))
        },
    )?;
    Ok(Box::new(move || {
        let mut entries: HashMap<(String, i64), MatchEntries> = HashMap::new();
        for (content_hash, position, barcode) in barcodes {
            entries
                .entry((content_hash, position))
                .or_default()
                .barcodes
                .push(barcode);
        }
        for (content_hash, position, max_edge, url) in cover_copies {
            entries
                .entry((content_hash, position))
                .or_default()
                .cover_copies
                .push(bae_catalogs::remote_image::DownscaledCopy { url, max_edge });
        }
        for (content_hash, position, medium) in media {
            entries
                .entry((content_hash, position))
                .or_default()
                .media
                .push(medium);
        }
        for (content_hash, position, catalog, key) in links {
            entries
                .entry((content_hash, position))
                .or_default()
                .links
                .push((catalog, key));
        }
        for (content_hash, position, link) in album_links {
            entries
                .entry((content_hash, position))
                .or_default()
                .album_links
                .push(link);
        }
        let mut matches: HashMap<String, StoredMatches> = HashMap::new();
        for row in rows {
            let columns = row?;
            // A match with nothing in the child tables has no rows there.
            let entries = entries
                .remove(&(columns.content_hash.clone(), columns.position))
                .unwrap_or_default();
            let row = match_of(columns, entries)?;
            let entry = matches.entry(row.content_hash).or_default();
            let list = if row.narrowed_out {
                &mut entry.narrowed_out
            } else {
                &mut entry.found
            };
            list.push(row.stored);
        }
        Ok(matches)
    }))
}

/// Every stored candidate row, or the one `only` names, assembled with the
/// verdict, signals, provenance, and file-decision rows that hang off it.
pub(crate) fn load_states_on(
    sql: &SqlReadContext<'_>,
    only: CandidateRows<'_>,
) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
    load_states_rows_on(sql, only)?()
}

pub(crate) fn load_states_rows_on(
    sql: &SqlReadContext<'_>,
    only: CandidateRows<'_>,
) -> Result<
    impl FnOnce() -> Result<HashMap<String, DbImportCandidateState>, DbError> + Send + 'static,
    DbError,
> {
    let filter = only.filter("content_hash");
    let clause = &filter.clause;
    let states = sql.query(
        &format!(
            "SELECT {STATE_COLUMNS} FROM import_candidate_state \
             {clause}"
        ),
        filter.params().as_slice(),
        |row| Ok(read_state_row(row)),
    )?;
    let verdicts = load_verdicts_on(sql, only)?;
    let edits = load_edits_on(sql, only)?;
    let signals = load_signals_on(sql, only)?;
    let provenances = load_provenance_rows_on(sql, only)?;
    let links = load_release_link_rows_on(sql, only)?;
    let authors = super::pane_rows::load_authors_on(sql, only)?;
    let lookup_choices = load_lookup_choices_on(sql, only)?;

    Ok(move || {
        let mut authors = authors;
        let mut verdicts = verdicts()?;
        let mut edits = edits()?;
        let mut signals = signals()?;
        let mut provenances = provenances()?;
        let mut links = links()?;
        let mut lookup_choices = lookup_choices()?;
        let mut out = HashMap::with_capacity(states.len());
        for state in states {
            let state = state?;
            let file_edits = edits.remove(&state.content_hash).ok_or_else(|| {
                DbError::Message(format!(
                    "candidate {}'s file decisions vanished while it was read",
                    state.content_hash
                ))
            })?;
            let metadata_revision = u64::try_from(state.metadata_revision).map_err(|error| {
                DbError::Message(format!(
                    "import candidate {} has a negative metadata revision: {error}",
                    state.content_hash
                ))
            })?;
            let provenance = provenances.remove(&state.content_hash);
            let author = authors.remove(&state.content_hash).ok_or_else(|| {
                DbError::Message(format!(
                    "candidate {} has no editable metadata draft",
                    state.content_hash
                ))
            })?;
            author
                .check_provenance(provenance.as_ref())
                .map_err(|error| {
                    DbError::Message(format!("candidate {}: {error}", state.content_hash))
                })?;
            out.insert(
                state.content_hash.clone(),
                DbImportCandidateState {
                    signals: signals.remove(&state.content_hash),
                    lookup_choices: lookup_choices
                        .remove(&state.content_hash)
                        .unwrap_or_default(),
                    identify: verdicts.remove(&state.content_hash),
                    metadata_author: author,
                    metadata_provenance: provenance,
                    release_link: links.remove(&state.content_hash),
                    content_hash: state.content_hash,
                    folder_path: state.folder_path,
                    file_edits,
                    metadata_revision,
                },
            );
        }
        Ok(out)
    })
}

/// The file decisions a read fetched, still to be assembled — boxed so a read
/// on either connection hands back one type.
type FinishEdits<T> = Box<dyn FnOnce() -> Result<T, DbError> + Send + 'static>;

/// The file decisions of every candidate, or of the one `only` names, each
/// at the revision its state row holds.
fn load_edits_on(
    sql: &(impl super::super::query::QueryOne + super::super::query::QueryRows),
    only: CandidateRows<'_>,
) -> Result<FinishEdits<HashMap<String, CandidateFileEdits>>, DbError> {
    let filter = only.filter("state.content_hash");
    let rows = sql.query(
        &format!(
            "SELECT state.content_hash, {FILE_EDITS_COLUMNS} \
             FROM import_candidate_state AS state {}",
            filter.clause
        ),
        filter.params().as_slice(),
        read_file_edits,
    )?;
    Ok(Box::new(move || {
        rows.into_iter()
            .map(|stored| Ok((stored.content_hash().to_string(), stored.edits()?)))
            .collect()
    }))
}

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
