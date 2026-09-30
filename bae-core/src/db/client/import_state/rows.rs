//! Reading a candidate back: its row, its verdict with the matches that hang
//! off it, its draft's provenance, its release link, and its file decisions.

use super::edit_rows::{apply_sheet_disc_row, read_sheet_disc_row};
use super::lookup_choice_rows::load_lookup_choices_on;
use super::signal_rows::load_signals_on;
use super::verdict_rows::{
    identification_of, match_of, read_match_row, read_verdict_row, unreadable, MatchEntries,
    StoredMedium,
    StoredMatches, VERDICT_COLUMNS,
};
use super::*;
use crate::import::{AlbumLink, Catalog, MetadataProvenance, MetadataRef, PressingLink, ReleaseLink};
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

/// Every candidate's draft provenance, or the one `only` names.
pub(crate) fn load_provenance_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<CandidateProvenances, DbError> {
    load_provenance_rows_on(sql, only)?()
}

pub(crate) fn load_provenance_rows_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<impl FnOnce() -> Result<CandidateProvenances, DbError> + Send + 'static, DbError> {
    let rows = sql.query(
        "SELECT content_hash, kind, source, release_id \
         FROM import_candidate_draft_provenance \
         WHERE :only IS NULL OR content_hash = :only",
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        },
    )?;
    Ok(move || {
        let mut out = HashMap::with_capacity(rows.len());
        for (content_hash, kind, source, release_id) in rows {
            let provenance = match kind.as_str() {
                "file_tags" => MetadataProvenance::FileMetadata,
                "external_release" => {
                    let missing = |what: &str| {
                        DbError::Message(format!(
                            "stored external metadata provenance names no {what}"
                        ))
                    };
                    MetadataProvenance::ExternalRelease {
                        record: MetadataRef::new(
                            Catalog::from_str(&source.ok_or_else(|| missing("catalog"))?)
                                .map_err(DbError::Message)?,
                            release_id.ok_or_else(|| missing("release"))?,
                        ),
                    }
                }
                other => return Err(unreadable("provenance kind", other)),
            };
            out.insert(content_hash, provenance);
        }
        Ok(out)
    })
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

/// Every candidate's release link, or the one `only` names, each with its
/// partners or its albums.
pub(crate) fn load_release_links_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<CandidateLinks, DbError> {
    load_release_link_rows_on(sql, only)?()
}

pub(crate) fn load_release_link_rows_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<impl FnOnce() -> Result<CandidateLinks, DbError> + Send + 'static, DbError> {
    let read = |row: &Row<'_>| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    };
    let partner_rows = sql.query(
        "SELECT content_hash, source, release_id FROM import_candidate_release_link_partner \
         WHERE :only IS NULL OR content_hash = :only \
         ORDER BY content_hash, source",
        named_params! { ":only": only },
        read,
    )?;
    let album_rows = sql.query(
        "SELECT content_hash, catalog, album_id FROM import_candidate_release_link_album \
         WHERE :only IS NULL OR content_hash = :only",
        named_params! { ":only": only },
        read,
    )?;
    let rows = sql.query(
        "SELECT content_hash, kind, source, release_id FROM import_candidate_release_link \
         WHERE :only IS NULL OR content_hash = :only",
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        },
    )?;
    Ok(move || {
        let reference = |catalog: String, key: String| {
            Ok::<_, DbError>(MetadataRef::new(
                Catalog::from_str(&catalog).map_err(DbError::Message)?,
                key,
            ))
        };
        let mut partners: HashMap<String, Vec<MetadataRef>> = HashMap::new();
        for (content_hash, source, release_id) in partner_rows {
            partners
                .entry(content_hash)
                .or_default()
                .push(reference(source, release_id)?);
        }
        let mut albums: HashMap<String, Vec<MetadataRef>> = HashMap::new();
        for (content_hash, catalog, album_id) in album_rows {
            albums
                .entry(content_hash)
                .or_default()
                .push(reference(catalog, album_id)?);
        }
        let mut out = HashMap::with_capacity(rows.len());
        for (content_hash, kind, source, release_id) in rows {
            let link = match (kind.as_str(), source, release_id) {
                ("pressing", Some(source), Some(release_id)) => {
                    ReleaseLink::Pressing(PressingLink {
                        record: reference(source, release_id)?,
                        partners: partners.remove(&content_hash).unwrap_or_default(),
                    })
                }
                ("album", None, None) => ReleaseLink::Album(
                    AlbumLink::new(albums.remove(&content_hash).unwrap_or_default())
                        .ok_or_else(|| {
                            DbError::Message(format!(
                                "{content_hash} is linked to an album no catalog names"
                            ))
                        })?,
                ),
                (other, _, _) => return Err(unreadable("release link", other)),
            };
            out.insert(content_hash, link);
        }
        Ok(out)
    })
}

/// Every candidate's verdict, or the one `only` names, each rebuilt with the
/// matches that hang off it.
pub(crate) fn load_verdicts_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<
    impl FnOnce() -> Result<HashMap<String, DbCandidateIdentifyResult>, DbError> + Send + 'static,
    DbError,
> {
    let matches = load_matches_rows_on(sql, only)?;
    let rows = sql.query(
        &format!(
            "SELECT {VERDICT_COLUMNS} FROM import_candidate_verdict \
             WHERE :only IS NULL OR content_hash = :only"
        ),
        named_params! { ":only": only },
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
    edit_revision: i64,
    metadata_revision: i64,
}

fn read_state_row(row: &Row<'_>) -> Result<StateRow, DbError> {
    Ok(StateRow {
        content_hash: row.get("content_hash")?,
        folder_path: row.get("folder_path")?,
        edit_revision: row.get("edit_revision")?,
        metadata_revision: row.get("metadata_revision")?,
    })
}

const STATE_COLUMNS: &str = "content_hash, folder_path, edit_revision, metadata_revision";

const MATCH_COLUMNS: &str = "content_hash, position, pressing, source, release_id, title, artist, \
     year, labels, country, region, status, packaging, discogs_details, \
     media_kind, cover_url, \
     cover_label, cover_source, cover_standing, source_group_id, album_links, source_tracks_kind, \
     source_tracks_count, \
     by_disc_id, by_barcode, by_catalog, by_isrc, by_search, by_pressing, narrowed_out, \
     document_failure, document_failure_status, album_first_year, \
     track_titles, notes, named_note";

const SHEET_DISC_COLUMNS: &str = "content_hash, sheet_id, disc, disc_number";

/// Every candidate's stored matches, keyed by content hash, or just the one
/// `only` names.
///
/// The whole row per match rather than a count and a lead, and the barcode,
/// medium and link rows that hang off it: how many *pressings* a verdict named
/// is decided by grouping them, which needs everything each one states. The
/// one reader of these columns, for the pane's whole verdict and for the
/// queue list's summary alike.
pub(crate) fn load_matches_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, StoredMatches>, DbError> {
    load_matches_rows_on(sql, only)?()
}

pub(crate) fn load_matches_rows_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<
    impl FnOnce() -> Result<HashMap<String, StoredMatches>, DbError> + Send + 'static,
    DbError,
> {
    let rows = sql.query(
        &format!(
            "SELECT {MATCH_COLUMNS} FROM import_candidate_match \
             WHERE :only IS NULL OR content_hash = :only \
             ORDER BY content_hash, position"
        ),
        named_params! { ":only": only },
        |row| Ok(read_match_row(row)),
    )?;
    let barcodes = sql.query(
        "SELECT content_hash, position, barcode FROM import_candidate_match_barcode \
         WHERE :only IS NULL OR content_hash = :only \
         ORDER BY content_hash, position, ordinal",
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )?;
    let cover_copies = sql.query(
        "SELECT content_hash, position, max_edge, url FROM import_candidate_match_cover_copy \
         WHERE :only IS NULL OR content_hash = :only",
        named_params! { ":only": only },
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
        "SELECT content_hash, position, media_kind, medium, quantity \
         FROM import_candidate_match_medium \
         WHERE :only IS NULL OR content_hash = :only \
         ORDER BY content_hash, position, ordinal",
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                StoredMedium {
                    kind: row.get(2)?,
                    medium: super::super::pressing_columns::keyed(
                        row,
                        "medium",
                        crate::pressing::Medium::from_key,
                    )?,
                    quantity: row.get(4)?,
                },
            ))
        },
    )?;
    let links = sql.query(
        "SELECT content_hash, position, catalog, key FROM import_candidate_match_link \
         WHERE :only IS NULL OR content_hash = :only \
         ORDER BY content_hash, position, ordinal",
        named_params! { ":only": only },
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
        "SELECT content_hash, position, catalog, key, stated, wikidata_item, \
                musicbrainz_release, release_catalog, release_key \
         FROM import_candidate_match_album_link \
         WHERE :only IS NULL OR content_hash = :only \
         ORDER BY content_hash, position, ordinal",
        named_params! { ":only": only },
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
    Ok(move || {
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
                .push(crate::import::cover_art::DownscaledCopy { url, max_edge });
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
    })
}

/// Every stored candidate row, or the one `only` names, assembled with the
/// verdict, signals, provenance, and file-decision rows that hang off it.
pub(crate) fn load_states_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
    load_states_rows_on(sql, only)?()
}

pub(crate) fn load_states_rows_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<
    impl FnOnce() -> Result<HashMap<String, DbImportCandidateState>, DbError> + Send + 'static,
    DbError,
> {
    let states = sql.query(
        &format!(
            "SELECT {STATE_COLUMNS} FROM import_candidate_state \
             WHERE :only IS NULL OR content_hash = :only"
        ),
        named_params! { ":only": only },
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
            let mut file_edits = edits.remove(&state.content_hash).unwrap_or_default();
            file_edits.revision = u64::try_from(state.edit_revision).map_err(|_| {
                DbError::Message(format!(
                    "import candidate {} has a negative edit revision",
                    state.content_hash
                ))
            })?;
            let metadata_revision = u64::try_from(state.metadata_revision).map_err(|_| {
                DbError::Message(format!(
                    "import candidate {} has a negative metadata revision",
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

/// The sheet decisions of every candidate, or of the one `only` names.
/// `CandidateFileEdits::revision` is left at zero — it lives on the state row,
/// which is what fills it in.
/// The file decisions a read fetched, still to be assembled — boxed so a read
/// on either connection hands back one type.
type FinishEdits<T> = Box<dyn FnOnce() -> Result<T, DbError> + Send + 'static>;

fn load_edits_on(
    sql: &(impl super::super::query::QueryOne + super::super::query::QueryRows),
    only: Option<&str>,
) -> Result<FinishEdits<HashMap<String, CandidateFileEdits>>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT {SHEET_DISC_COLUMNS} FROM import_candidate_sheet_disc \
             WHERE :only IS NULL OR content_hash = :only \
             ORDER BY content_hash, sheet_id"
        ),
        named_params! { ":only": only },
        |row| Ok(read_sheet_disc_row(row)),
    )?;
    let references = sql.query(
        "SELECT content_hash, sheet_id, file_reference, file_id FROM import_candidate_sheet_reference WHERE :only IS NULL OR content_hash = :only ORDER BY content_hash, sheet_id, file_reference",
        named_params! { ":only": only },
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?)),
    )?;
    Ok(Box::new(move || {
        let mut edits: HashMap<String, CandidateFileEdits> = HashMap::new();
        for row in rows {
            let row = row?;
            let entry = edits.entry(row.content_hash.clone()).or_default();
            apply_sheet_disc_row(entry, row)?;
        }
        for (hash, sheet, reference, file) in references {
            use crate::import::folder_scanner::UserSheetBinding;
            let decision = match file {
                Some(file_id) => UserSheetBinding::Describes { file_id },
                None => UserSheetBinding::Cleared,
            };
            edits
                .entry(hash)
                .or_default()
                .sheet_bindings
                .set_reference(sheet, reference, decision);
        }
        Ok(edits)
    }))
}

/// One candidate's file decisions, revision included. Progressive scans call
/// this after they compute a content hash, so each emitted row performs one
/// indexed lookup instead of rereading the whole table.
pub(crate) fn load_candidate_file_edits_on(
    sql: &(impl super::super::query::QueryOne + super::super::query::QueryRows),
    content_hash: &str,
) -> Result<FinishEdits<CandidateFileEdits>, DbError> {
    let revision: Option<i64> = sql
        .query_row(
            "SELECT edit_revision FROM import_candidate_state WHERE content_hash = ?",
            [content_hash],
            |row| row.get(0),
        )
        .optional()?;
    let edits = if revision.is_some() {
        Some(load_edits_on(sql, Some(content_hash))?)
    } else {
        None
    };
    let content_hash = content_hash.to_string();
    Ok(Box::new(move || {
        let Some(revision) = revision else {
            return Ok(CandidateFileEdits::default());
        };
        let mut edits = edits.expect("a stored revision has fetched file edits")()?
            .remove(&content_hash)
            .unwrap_or_default();
        edits.revision = u64::try_from(revision).map_err(|_| {
            DbError::Message(format!(
                "import candidate {content_hash} has a negative edit revision"
            ))
        })?;
        Ok(edits)
    }))
}
