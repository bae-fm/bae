//! The identify verdict as its own row, its matches, and the read back.
//!
//! One `import_candidate_verdict` row holds a whole terminal verdict — found,
//! nothing anywhere, manual only, or failed — and the ordered release-match
//! rows hang off it. Deleting the row clears the matches with it, so a
//! candidate can neither hold two verdicts nor keep matches without one.

use super::*;
use super::super::album_link_rows::{AlbumLinkRow, StatementColumns};
use crate::identify::{MediumConflict, 
    Findings, IdentifyFailure, IdentifyRunView, LookupProvenance, NarrowedOut, TerminalVerdict,
};
use crate::import::album_links::AlbumLinks;
use crate::import::cover_art::{CoverStanding, DownscaledCopy, RemoteCover, RemoteImageSet};
use crate::import::search::{MetadataResult, SourceTracks};
use crate::pressing::{Medium, StatedFormat, StatedMedia};
use crate::import::{Catalog, MetadataRef};
use std::str::FromStr;

/// What a stored column holds that no writer here produces.
pub(super) fn unreadable(column: &str, stored: &str) -> DbError {
    DbError::Message(format!("import candidate column {column} holds {stored:?}"))
}

fn source_of(stored: &str) -> Result<Catalog, DbError> {
    Catalog::from_str(stored).map_err(DbError::Message)
}

fn standing_of(stored: &str) -> Result<CoverStanding, DbError> {
    CoverStanding::from_column(stored).ok_or_else(|| unreadable("cover_standing", stored))
}

/// Clear whatever verdict stands under `content_hash`. The match rows go with
/// it: they hang off the verdict row.
pub(super) fn delete_verdict(sql: &SqlContext<'_, '_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_verdict WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

/// Write one whole verdict and the matches it found. The caller has already
/// cleared whatever stood under this hash.
pub(super) fn insert_verdict(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    identification: &DbCandidateIdentifyResult,
) -> Result<(), DbError> {
    let verdict = &identification.verdict;
    let (kind, track_count) = match verdict {
        TerminalVerdict::Found { track_count, .. } => ("found", Some(*track_count)),
        TerminalVerdict::NotFoundAnywhere { .. } => ("not_found", None),
        TerminalVerdict::ManualOnly { track_count, .. } => ("manual_only", Some(*track_count)),
        TerminalVerdict::Failed { track_count, .. } => ("failed", Some(*track_count)),
    };
    // The ledger the run recorded, stored whole: no query reads into it, and
    // what it draws is the run laid out cell by cell.
    let ledger_json = verdict
        .ledger()
        .map(|ledger| {
            serde_json::to_string(ledger).map_err(|error| {
                DbError::Message(format!(
                    "failed to serialize the identify ledger for {content_hash}: {error}"
                ))
            })
        })
        .transpose()?;
    let (medium_conflict, medium_conflict_sample_rate_hz) =
        match verdict.findings().and_then(|findings| findings.medium_conflict) {
            None => (None, None),
            Some(MediumConflict::CdRip) => (Some("cd_rip"), None),
            Some(MediumConflict::NotCdAudio { sample_rate_hz }) => {
                (Some("not_cd_audio"), Some(sample_rate_hz))
            }
            Some(MediumConflict::MonoAudio) => (Some("mono_audio"), None),
        };
    let failures_json = match verdict {
        TerminalVerdict::Failed { failures, .. } => {
            if failures.is_empty() {
                return Err(DbError::Message(format!(
                    "failed verdict for {content_hash} contains no failed lookup"
                )));
            }
            Some(serde_json::to_string(failures).map_err(|error| {
                DbError::Message(format!(
                    "failed to serialize identify failure for {content_hash}: {error}"
                ))
            })?)
        }
        TerminalVerdict::Found { .. }
        | TerminalVerdict::NotFoundAnywhere { .. }
        | TerminalVerdict::ManualOnly { .. } => None,
    };
    sql.execute(
        "INSERT INTO import_candidate_verdict \
             (content_hash, kind, track_count, failures_json, \
              ledger_json, identified_at, medium_conflict, medium_conflict_sample_rate_hz) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            content_hash,
            kind,
            track_count,
            failures_json,
            ledger_json,
            identification.identified_at.to_rfc3339(),
            medium_conflict,
            medium_conflict_sample_rate_hz,
        ],
    )?;
    insert_matches(sql, content_hash, verdict)
}

/// The releases of one verdict, written under the verdict row that found them:
/// the matches first, then the ones agreement narrowed out, which continue the
/// same position sequence and are marked as what they are. A found verdict and
/// a failed one write theirs alike; the other two hold none.
fn insert_matches(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    verdict: &TerminalVerdict,
) -> Result<(), DbError> {
    let Some(Findings {
        matches,
        provenance,
        pressings,
        narrowed_out,
        medium_conflict: _,
    }) = verdict.findings()
    else {
        return Ok(());
    };
    let aligned = |what: &str, results: &[MetadataResult], provenance: &[LookupProvenance]| {
        if results.len() == provenance.len() {
            return Ok(());
        }
        Err(DbError::Message(format!(
            "a verdict for {content_hash} carries {} {what} and {} provenance entries; they \
             are index-aligned",
            results.len(),
            provenance.len()
        )))
    };
    aligned("matches", matches, provenance)?;
    aligned(
        "narrowed-out releases",
        &narrowed_out.matches,
        &narrowed_out.provenance,
    )?;
    let rowed = |what: &str, results: &[MetadataResult], rows: &[u32]| {
        if results.len() == rows.len() {
            return Ok(());
        }
        Err(DbError::Message(format!(
            "a verdict for {content_hash} carries {} {what} and {} pressing entries; they are \
             index-aligned",
            results.len(),
            rows.len()
        )))
    };
    rowed("matches", matches, pressings)?;
    rowed(
        "narrowed-out releases",
        &narrowed_out.matches,
        &narrowed_out.pressings,
    )?;
    let written = matches
        .iter()
        .zip(provenance.iter())
        .zip(pressings.iter())
        .map(|pair| (pair, false))
        .chain(
            narrowed_out
                .matches
                .iter()
                .zip(narrowed_out.provenance.iter())
                .zip(narrowed_out.pressings.iter())
                .map(|pair| (pair, true)),
        );
    for (position, (((result, provenance), pressing), narrowed_out)) in written.enumerate() {
        insert_match(
            sql,
            content_hash,
            position,
            *pressing,
            result,
            provenance,
            narrowed_out,
        )?;
    }
    Ok(())
}

fn insert_match(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    position: usize,
    pressing: u32,
    result: &MetadataResult,
    provenance: &LookupProvenance,
    narrowed_out: bool,
) -> Result<(), DbError> {
    let position = i64::try_from(position)
        .map_err(|_| DbError::Message("a match list is longer than SQLite counts".to_string()))?;
    let cover = result.cover_art.as_ref();
    let (tracks_kind, tracks_count) = match &result.source_tracks {
        None => (None, None),
        Some(SourceTracks::Nothing) => (Some("nothing"), None),
        Some(SourceTracks::Listed { count }) => (Some("listed"), Some(i64::from(*count))),
    };
    let album_links_kind = match &result.album_links {
        AlbumLinks::NotAsked => ALBUM_LINKS_NOT_ASKED,
        AlbumLinks::Read(_) => ALBUM_LINKS_READ,
        AlbumLinks::Unread => ALBUM_LINKS_UNREAD,
    };
    let (media_kind, media_entries): (&str, Vec<(Option<Medium>, u32)>) = match &result.media {
        StatedMedia::Undescribed => (MEDIA_UNDESCRIBED, Vec::new()),
        StatedMedia::PerMedium(entries) => (
            MEDIA_PER_MEDIUM,
            entries.iter().map(|medium| (*medium, 1)).collect(),
        ),
        StatedMedia::Formats(formats) => (
            MEDIA_FORMATS,
            formats
                .iter()
                .map(|format| (format.medium, format.quantity))
                .collect(),
        ),
    };
    let facts = super::super::pressing_columns::FactColumns::of(&result.facts());
    sql.execute(
        "INSERT INTO import_candidate_match \
             (content_hash, position, pressing, source, release_id, title, artist, year, \
              label, catalog_number, country, region, status, packaging, discogs_details, \
              media_kind, cover_url, cover_label, cover_source, \
              cover_standing, source_group_id, album_links, source_tracks_kind, \
              source_tracks_count, by_disc_id, by_barcode, by_catalog, by_search, \
              named_by_catalog, named_by_key, narrowed_out) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
                 ?, ?, ?, ?)",
        params![
            content_hash,
            position,
            pressing,
            result.source.as_str(),
            result.release_id,
            result.title,
            result.artist,
            result.year,
            result.label,
            result.catalog_number,
            facts.country,
            facts.region,
            facts.status,
            facts.packaging,
            facts.discogs_details,
            media_kind,
            cover.map(|cover| cover.image.url.as_str()),
            cover.map(|cover| cover.label.as_str()),
            cover.map(|cover| cover.source.as_str()),
            cover.map(|cover| cover.standing.as_str()),
            result.source_group_id,
            album_links_kind,
            tracks_kind,
            tracks_count,
            provenance.by_disc_id,
            provenance.by_barcode,
            provenance.by_catalog,
            provenance.by_search,
            provenance.named_by.as_ref().map(|by| by.catalog.as_str()),
            provenance.named_by.as_ref().map(|by| by.key.as_str()),
            narrowed_out,
        ],
    )?;
    if let Some(cover) = cover {
        for copy in &cover.image.downscaled {
            sql.execute(
                "INSERT INTO import_candidate_match_cover_copy \
                     (content_hash, position, cover_url, max_edge, url) \
                 VALUES (?, ?, ?, ?, ?)",
                params![content_hash, position, cover.image.url, copy.max_edge, copy.url],
            )?;
        }
    }
    for (ordinal, barcode) in result.barcodes.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_match_barcode (content_hash, position, ordinal, barcode) \
             VALUES (?, ?, ?, ?)",
            params![content_hash, position, ordinal_column(ordinal)?, barcode],
        )?;
    }
    for (ordinal, (medium, quantity)) in media_entries.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_match_medium \
                 (content_hash, position, media_kind, ordinal, medium, quantity) \
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                position,
                media_kind,
                ordinal_column(ordinal)?,
                medium.map(Medium::key),
                quantity,
            ],
        )?;
    }
    for (ordinal, link) in result.links.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_match_link (content_hash, position, ordinal, catalog, key) \
             VALUES (?, ?, ?, ?, ?)",
            params![
                content_hash,
                position,
                ordinal_column(ordinal)?,
                link.catalog.as_str(),
                link.key
            ],
        )?;
    }
    for (ordinal, link) in result.album_links.read().iter().enumerate() {
        let StatementColumns {
            stated,
            wikidata_item,
            musicbrainz_release,
            twin,
        } = StatementColumns::of(&link.stated);
        sql.execute(
            "INSERT INTO import_candidate_match_album_link \
                 (content_hash, position, ordinal, catalog, key, stated, wikidata_item, \
                  musicbrainz_release, twin_catalog, twin_key) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                position,
                ordinal_column(ordinal)?,
                link.album.catalog.as_str(),
                link.album.key,
                stated,
                wikidata_item,
                musicbrainz_release,
                twin.map(|twin| twin.catalog.as_str()),
                twin.map(|twin| twin.key.as_str()),
            ],
        )?;
    }
    Ok(())
}

fn ordinal_column(ordinal: usize) -> Result<i64, DbError> {
    i64::try_from(ordinal)
        .map_err(|_| DbError::Message("a match's list is longer than SQLite counts".to_string()))
}

/// The stored `album_links` values, one per [`AlbumLinks`] shape.
const ALBUM_LINKS_NOT_ASKED: &str = "not_asked";
const ALBUM_LINKS_READ: &str = "read";
const ALBUM_LINKS_UNREAD: &str = "unread";

/// The stored `media_kind` values, one per [`StatedMedia`] shape.
pub(super) const MEDIA_UNDESCRIBED: &str = "undescribed";
pub(super) const MEDIA_PER_MEDIUM: &str = "per_medium";
pub(super) const MEDIA_FORMATS: &str = "formats";

/// One medium row of a match: the media kind it was written for, the carrier
/// it names and how many of it.
pub(super) struct StoredMedium {
    pub(super) kind: String,
    pub(super) medium: Option<Medium>,
    pub(super) quantity: u32,
}

/// The ordinal-ordered rows the child tables hold for one match, read in the
/// same pass as its row and handed to [`match_of`].
#[derive(Default)]
pub(super) struct MatchEntries {
    pub(super) barcodes: Vec<String>,
    /// The downscaled copies of the match's cover.
    pub(super) cover_copies: Vec<DownscaledCopy>,
    /// The kind rides on every medium row, so a row whose kind disagrees with
    /// its match's is unreadable rather than silently reinterpreted.
    pub(super) media: Vec<StoredMedium>,
    pub(super) links: Vec<(String, String)>,
    /// The album link rows, for a match whose album links were read.
    pub(super) album_links: Vec<AlbumLinkRow>,
}

/// One stored release of a verdict: what the lookup returned, which lookups
/// named it, and which pressing row of its list the run put it in.
pub(crate) struct StoredMatch {
    pub(crate) result: MetadataResult,
    pub(crate) provenance: LookupProvenance,
    /// The row's number within this list, as the run numbered it. Rows are
    /// read, never re-formed: a list of a run's answers does not hold what
    /// the run decided its rows against.
    pub(crate) pressing: u32,
}

/// One candidate's stored releases, each list in the order it was written: the
/// verdict's matches, the lead first, and the releases its signals' agreement
/// narrowed out.
#[derive(Default)]
pub(crate) struct StoredMatches {
    pub(crate) found: Vec<StoredMatch>,
    pub(crate) narrowed_out: Vec<StoredMatch>,
}

pub(super) struct MatchRow {
    pub(super) content_hash: String,
    pub(super) stored: StoredMatch,
    /// Whether this release is one the agreement left out rather than one the
    /// verdict settled on.
    pub(super) narrowed_out: bool,
}

/// One match row's own columns, before the child tables' rows are joined
/// to it.
pub(super) struct MatchColumns {
    pub(super) content_hash: String,
    pub(super) position: i64,
    pressing: i64,
    media_kind: String,
    /// The stored `album_links` value; [`match_of`] reads it with the album
    /// link rows.
    album_links: String,
    /// The result with its list fields still empty; [`match_of`] fills them.
    result: MetadataResult,
    provenance: LookupProvenance,
    narrowed_out: bool,
}

/// The match its columns and its child rows describe. The media kind and the
/// medium rows have to agree: a kind with no entries is `undescribed`, a
/// `per_medium` row counts one medium, and a row of another kind is a
/// different match's.
pub(super) fn match_of(columns: MatchColumns, entries: MatchEntries) -> Result<MatchRow, DbError> {
    let MatchColumns {
        content_hash,
        position,
        pressing,
        media_kind,
        album_links: album_links_kind,
        mut result,
        provenance,
        narrowed_out,
    } = columns;
    let MatchEntries {
        barcodes,
        cover_copies,
        media,
        links,
        album_links,
    } = entries;
    let mismatch = |what: &str| {
        DbError::Message(format!(
            "match {position} of {content_hash} holds {media_kind} media but {what}"
        ))
    };
    for stored in &media {
        if stored.kind != media_kind {
            return Err(mismatch(&format!("a {} medium row", stored.kind)));
        }
    }
    result.media = match media_kind.as_str() {
        MEDIA_UNDESCRIBED => {
            if !media.is_empty() {
                return Err(mismatch("lists medium rows"));
            }
            StatedMedia::Undescribed
        }
        MEDIA_PER_MEDIUM => StatedMedia::PerMedium(
            media
                .into_iter()
                .map(|stored| {
                    (stored.quantity == 1)
                        .then_some(stored.medium)
                        .ok_or_else(|| mismatch("a medium row counts more than one"))
                })
                .collect::<Result<_, _>>()?,
        ),
        MEDIA_FORMATS => StatedMedia::Formats(
            media
                .into_iter()
                .map(|stored| StatedFormat {
                    medium: stored.medium,
                    quantity: stored.quantity,
                })
                .collect(),
        ),
        other => return Err(unreadable("media_kind", other)),
    };
    result.barcodes = barcodes;
    // Copy rows reference their match's cover, so a match without one has none.
    if let Some(cover) = &mut result.cover_art {
        cover.image = RemoteImageSet::with_copies(cover.image.url.clone(), cover_copies);
    }
    result.links = links
        .into_iter()
        .map(|(catalog, key)| Ok(MetadataRef::new(source_of(&catalog)?, key)))
        .collect::<Result<_, DbError>>()?;
    result.album_links = match album_links_kind.as_str() {
        ALBUM_LINKS_READ => AlbumLinks::Read(
            album_links
                .into_iter()
                .map(AlbumLinkRow::link)
                .collect::<Result<_, DbError>>()?,
        ),
        ALBUM_LINKS_NOT_ASKED | ALBUM_LINKS_UNREAD if !album_links.is_empty() => {
            return Err(DbError::Message(format!(
                "match {position} of {content_hash} holds album link rows but its album links \
                 are {album_links_kind}"
            )))
        }
        ALBUM_LINKS_NOT_ASKED => AlbumLinks::NotAsked,
        ALBUM_LINKS_UNREAD => AlbumLinks::Unread,
        other => return Err(unreadable("album_links", other)),
    };
    let row = u32::try_from(pressing).map_err(|_| {
        DbError::Message(format!(
            "match {position} of {content_hash} names pressing row {pressing}"
        ))
    })?;
    Ok(MatchRow {
        content_hash,
        stored: StoredMatch {
            result,
            provenance,
            pressing: row,
        },
        narrowed_out,
    })
}

pub(super) fn read_match_row(row: &Row<'_>) -> Result<MatchColumns, DbError> {
    read_match_columns(row, row.get("pressing")?)
}

/// One match row's own columns with the pressing row it belongs to supplied
/// beside it, for a reader whose rows do not carry the column yet.
fn read_match_columns(row: &Row<'_>, pressing: i64) -> Result<MatchColumns, DbError> {
    let cover_url: Option<String> = row.get("cover_url")?;
    let cover_source: Option<String> = row.get("cover_source")?;
    let cover_art = match (cover_url, cover_source) {
        // The copies are child rows; [`match_of`] adds them.
        (Some(url), Some(source)) => Some(RemoteCover {
            image: RemoteImageSet::original(url),
            label: row.get("cover_label")?,
            source: source_of(&source)?,
            standing: standing_of(&row.get::<_, String>("cover_standing")?)?,
        }),
        // The table sets and clears the four cover columns together.
        _ => None,
    };
    let tracks_kind: Option<String> = row.get("source_tracks_kind")?;
    let source_tracks = match tracks_kind.as_deref() {
        None => None,
        Some("nothing") => Some(SourceTracks::Nothing),
        Some("listed") => {
            let count: i64 = row
                .get::<_, Option<i64>>("source_tracks_count")?
                .ok_or_else(|| {
                    DbError::Message("a listed source tracklist states no count".to_string())
                })?;
            Some(SourceTracks::Listed {
                count: u32::try_from(count).map_err(|_| {
                    DbError::Message("a source tracklist's count is out of range".to_string())
                })?,
            })
        }
        Some(other) => return Err(unreadable("source_tracks_kind", other)),
    };
    let source: String = row.get("source")?;
    let facts = super::super::pressing_columns::read_facts_without_media(row, "")?;
    Ok(MatchColumns {
        content_hash: row.get("content_hash")?,
        position: row.get("position")?,
        pressing,
        media_kind: row.get("media_kind")?,
        album_links: row.get("album_links")?,
        result: MetadataResult {
            source: source_of(&source)?,
            release_id: row.get("release_id")?,
            title: row.get("title")?,
            artist: row.get("artist")?,
            year: row.get("year")?,
            label: row.get("label")?,
            catalog_number: row.get("catalog_number")?,
            area: facts.area,
            status: facts.status,
            packaging: facts.packaging,
            discogs_details: facts.discogs_details,
            barcodes: Vec::new(),
            media: StatedMedia::Undescribed,
            links: Vec::new(),
            cover_art,
            source_group_id: row.get("source_group_id")?,
            album_links: AlbumLinks::NotAsked,
            source_tracks,
        },
        provenance: LookupProvenance {
            by_disc_id: row.get("by_disc_id")?,
            by_barcode: row.get("by_barcode")?,
            by_catalog: row.get("by_catalog")?,
            by_search: row.get("by_search")?,
            named_by: match (
                row.get::<_, Option<String>>("named_by_catalog")?,
                row.get::<_, Option<String>>("named_by_key")?,
            ) {
                (Some(catalog), Some(key)) => Some(MetadataRef::new(source_of(&catalog)?, key)),
                (None, None) => None,
                _ => {
                    return Err(DbError::Message(
                        "a match names half of the release that named it".to_string(),
                    ))
                }
            },
        },
        narrowed_out: row.get("narrowed_out")?,
    })
}

/// The columns of one stored verdict row.
pub(super) struct VerdictRow {
    pub(super) content_hash: String,
    pub(super) kind: String,
    pub(super) track_count: Option<i64>,
    pub(super) failures_json: Option<String>,
    pub(super) ledger_json: Option<String>,
    pub(super) identified_at: DateTime<Utc>,
    pub(super) medium_conflict: Option<MediumConflict>,
}

pub(super) const VERDICT_COLUMNS: &str = "content_hash, kind, track_count, \
     failures_json, ledger_json, identified_at, medium_conflict, \
     medium_conflict_sample_rate_hz";


pub(super) fn read_verdict_row(row: &Row<'_>) -> Result<VerdictRow, DbError> {
    Ok(VerdictRow {
        content_hash: row.get("content_hash")?,
        kind: row.get("kind")?,
        track_count: row.get("track_count")?,
        failures_json: row.get("failures_json")?,
        ledger_json: row.get("ledger_json")?,
        identified_at: super::rfc3339_column(row, "identified_at")?,
        medium_conflict: super::medium_conflict_of(
            row.get("medium_conflict")?,
            row.get("medium_conflict_sample_rate_hz")?,
        )?,
    })
}

/// Rebuild what identification concluded from its row and the matches it found.
pub(super) fn identification_of(
    row: VerdictRow,
    found: StoredMatches,
) -> Result<DbCandidateIdentifyResult, DbError> {
    let VerdictRow {
        content_hash,
        kind,
        track_count,
        failures_json,
        ledger_json,
        identified_at,
        medium_conflict,
    } = row;
    let ledger: Option<IdentifyRunView> = ledger_json
        .map(|json| {
            serde_json::from_str(&json).map_err(|error| {
                DbError::Message(format!(
                    "the identify ledger for {content_hash} is unreadable: {error}"
                ))
            })
        })
        .transpose()?;
    let count_of = || {
        track_count
            .ok_or_else(|| {
                DbError::Message(format!(
                    "verdict {kind} for {content_hash} counts no tracks"
                ))
            })
            .and_then(|count| {
                u32::try_from(count).map_err(|_| {
                    DbError::Message(format!(
                        "verdict {kind} for {content_hash} counts {count} tracks"
                    ))
                })
            })
    };
    // Only the verdicts that hold findings may have match rows under them; a
    // row under any other kind is one no writer here produces.
    let findings_of = |found: StoredMatches| {
        let (matches, provenance, pressings) = unzip_stored(found.found);
        let (narrowed_matches, narrowed_provenance, narrowed_pressings) =
            unzip_stored(found.narrowed_out);
        Findings {
            matches,
            provenance,
            pressings,
            narrowed_out: NarrowedOut {
                matches: narrowed_matches,
                provenance: narrowed_provenance,
                pressings: narrowed_pressings,
            },
            medium_conflict,
        }
    };
    let no_matches = |found: &StoredMatches| {
        if found.found.is_empty() && found.narrowed_out.is_empty() {
            return Ok(());
        }
        Err(DbError::Message(format!(
            "verdict {kind} for {content_hash} holds match rows"
        )))
    };
    let verdict = match kind.as_str() {
        "found" => TerminalVerdict::Found {
            findings: findings_of(found),
            track_count: count_of()?,
            ledger,
        },
        "not_found" => {
            no_matches(&found)?;
            TerminalVerdict::NotFoundAnywhere { ledger }
        }
        "manual_only" => {
            no_matches(&found)?;
            TerminalVerdict::ManualOnly {
                track_count: count_of()?,
                ledger,
            }
        }
        "failed" => {
            let json = failures_json.ok_or_else(|| {
                DbError::Message(format!(
                    "failed verdict for {content_hash} lists no failure"
                ))
            })?;
            let failures: Vec<IdentifyFailure> = serde_json::from_str(&json).map_err(|error| {
                DbError::Message(format!(
                    "identify failure for {content_hash} is unreadable: {error}"
                ))
            })?;
            if failures.is_empty() {
                return Err(DbError::Message(format!(
                    "identify failure for {content_hash} contains no failed lookup"
                )));
            }
            TerminalVerdict::Failed {
                failures,
                findings: findings_of(found),
                track_count: count_of()?,
                ledger,
            }
        }
        other => return Err(unreadable("verdict kind", other)),
    };
    Ok(DbCandidateIdentifyResult {
        verdict,
        identified_at,
    })
}

/// One stored list as the verdict carries it: three index-aligned lists.
fn unzip_stored(
    stored: Vec<StoredMatch>,
) -> (Vec<MetadataResult>, Vec<LookupProvenance>, Vec<u32>) {
    let mut results = Vec::with_capacity(stored.len());
    let mut provenance = Vec::with_capacity(stored.len());
    let mut pressings = Vec::with_capacity(stored.len());
    for entry in stored {
        results.push(entry.result);
        provenance.push(entry.provenance);
        pressings.push(entry.pressing);
    }
    (results, provenance, pressings)
}
