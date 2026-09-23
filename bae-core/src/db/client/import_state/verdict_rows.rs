//! The identify verdict as its own row, its matches, and the read back.
//!
//! One `import_candidate_verdict` row holds a whole terminal verdict — found,
//! nothing anywhere, manual only, or failed — and the ordered release-match
//! rows hang off it. Deleting the row clears the matches with it, so a
//! candidate can neither hold two verdicts nor keep matches without one.

use super::*;
use crate::identify::{IdentifyFailure, IdentifyRunView, LookupProvenance, TerminalVerdict};
use crate::import::cover_art::RemoteCover;
use crate::import::search::{MetadataResult, SourceTracks, StatedMedia};
use crate::import::{Catalog, MetadataRef};
use std::str::FromStr;

/// What a stored column holds that no writer here produces.
pub(super) fn unreadable(column: &str, stored: &str) -> DbError {
    DbError::Message(format!("import candidate column {column} holds {stored:?}"))
}

fn source_of(stored: &str) -> Result<Catalog, DbError> {
    Catalog::from_str(stored).map_err(DbError::Message)
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

/// Whether the result stored under `content_hash` is still unread, or `None`
/// with no result stored.
pub(super) fn stored_unread(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
) -> Result<Option<bool>, DbError> {
    Ok(sql
        .query_row(
            "SELECT unread FROM import_candidate_verdict WHERE content_hash = ?",
            [content_hash],
            |row| row.get::<_, bool>(0),
        )
        .optional()?)
}

impl Database {
    /// Mark the result stored for the candidate at `candidate_key` read: the
    /// person has opened it.
    pub(crate) async fn mark_candidate_result_read(
        &self,
        candidate_key: &str,
    ) -> Result<(), DbError> {
        let candidate_key = candidate_key.to_string();
        self.call(move |sql| {
            sql.execute(
                "UPDATE import_candidate_verdict SET unread = 0 \
                 WHERE unread = 1 AND content_hash IN \
                     (SELECT content_hash FROM scan_candidate WHERE path = ?)",
                [candidate_key],
            )?;
            Ok(())
        })
        .await
    }
}

/// Write one whole verdict and the matches it found, `unread` or not. The
/// caller has already cleared whatever stood under this hash.
pub(super) fn insert_verdict(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    identification: &DbCandidateIdentifyResult,
    unread: bool,
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
              ledger_json, identified_at, unread) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![
            content_hash,
            kind,
            track_count,
            failures_json,
            ledger_json,
            identification.identified_at.to_rfc3339(),
            unread,
        ],
    )?;
    insert_matches(sql, content_hash, verdict)
}

/// The releases of one verdict, written under the verdict row that found them:
/// the matches first, then the ones agreement narrowed out, which continue the
/// same position sequence and are marked as what they are.
fn insert_matches(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    verdict: &TerminalVerdict,
) -> Result<(), DbError> {
    match verdict {
        TerminalVerdict::Found {
            matches,
            provenance,
            pressings,
            narrowed_out,
            narrowed_out_provenance,
            narrowed_out_pressings,
            ..
        } => {
            let aligned =
                |what: &str, results: &[MetadataResult], provenance: &[LookupProvenance]| {
                    if results.len() == provenance.len() {
                        return Ok(());
                    }
                    Err(DbError::Message(format!(
                        "a found verdict for {content_hash} carries {} {what} and {} provenance \
                     entries; they are index-aligned",
                        results.len(),
                        provenance.len()
                    )))
                };
            aligned("matches", matches, provenance)?;
            aligned(
                "narrowed-out releases",
                narrowed_out,
                narrowed_out_provenance,
            )?;
            let rowed = |what: &str, results: &[MetadataResult], rows: &[u32]| {
                if results.len() == rows.len() {
                    return Ok(());
                }
                Err(DbError::Message(format!(
                    "a found verdict for {content_hash} carries {} {what} and {} pressing \
                     entries; they are index-aligned",
                    results.len(),
                    rows.len()
                )))
            };
            rowed("matches", matches, pressings)?;
            rowed(
                "narrowed-out releases",
                narrowed_out,
                narrowed_out_pressings,
            )?;
            let written = matches
                .iter()
                .zip(provenance.iter())
                .zip(pressings.iter())
                .map(|pair| (pair, false))
                .chain(
                    narrowed_out
                        .iter()
                        .zip(narrowed_out_provenance.iter())
                        .zip(narrowed_out_pressings.iter())
                        .map(|pair| (pair, true)),
                );
            for (position, (((result, provenance), pressing), narrowed_out)) in written.enumerate()
            {
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
        }
        TerminalVerdict::NotFoundAnywhere { .. }
        | TerminalVerdict::ManualOnly { .. }
        | TerminalVerdict::Failed { .. } => {}
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
    let (media_kind, media_entries): (&str, Vec<Option<&str>>) = match &result.media {
        StatedMedia::Undescribed => (MEDIA_UNDESCRIBED, Vec::new()),
        StatedMedia::PerMedium(entries) => (
            MEDIA_PER_MEDIUM,
            entries.iter().map(Option::as_deref).collect(),
        ),
        StatedMedia::Descriptors(tokens) => (
            MEDIA_DESCRIPTORS,
            tokens.iter().map(|token| Some(token.as_str())).collect(),
        ),
    };
    sql.execute(
        "INSERT INTO import_candidate_match \
             (content_hash, position, pressing, source, release_id, title, artist, year, format, \
              label, catalog_number, country, media_kind, cover_url, cover_thumbnail_url, \
              cover_label, cover_source, source_group_id, source_tracks_kind, \
              source_tracks_count, by_disc_id, by_barcode, by_catalog, by_search, narrowed_out) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            content_hash,
            position,
            pressing,
            result.source.as_str(),
            result.release_id,
            result.title,
            result.artist,
            result.year,
            result.format,
            result.label,
            result.catalog_number,
            result.country,
            media_kind,
            cover.map(|cover| cover.url.as_str()),
            cover.map(|cover| cover.thumbnail_url.as_str()),
            cover.map(|cover| cover.label.as_str()),
            cover.map(|cover| cover.source.as_str()),
            result.source_group_id,
            tracks_kind,
            tracks_count,
            provenance.by_disc_id,
            provenance.by_barcode,
            provenance.by_catalog,
            provenance.by_search,
            narrowed_out,
        ],
    )?;
    for (ordinal, barcode) in result.barcodes.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_match_barcode (content_hash, position, ordinal, barcode) \
             VALUES (?, ?, ?, ?)",
            params![content_hash, position, ordinal_column(ordinal)?, barcode],
        )?;
    }
    for (ordinal, format) in media_entries.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_match_medium \
                 (content_hash, position, media_kind, ordinal, format) \
             VALUES (?, ?, ?, ?, ?)",
            params![
                content_hash,
                position,
                media_kind,
                ordinal_column(ordinal)?,
                format
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
    Ok(())
}

fn ordinal_column(ordinal: usize) -> Result<i64, DbError> {
    i64::try_from(ordinal)
        .map_err(|_| DbError::Message("a match's list is longer than SQLite counts".to_string()))
}

/// The stored `media_kind` values, one per [`StatedMedia`] shape.
pub(super) const MEDIA_UNDESCRIBED: &str = "undescribed";
pub(super) const MEDIA_PER_MEDIUM: &str = "per_medium";
pub(super) const MEDIA_DESCRIPTORS: &str = "descriptors";

/// The ordinal-ordered rows the child tables hold for one match, read in the
/// same pass as its row and handed to [`match_of`].
#[derive(Default)]
pub(super) struct MatchEntries {
    pub(super) barcodes: Vec<String>,
    /// `(media_kind, format)`: the kind rides on every medium row, so a row
    /// whose kind disagrees with its match's is unreadable rather than
    /// silently reinterpreted.
    pub(super) media: Vec<(String, Option<String>)>,
    pub(super) links: Vec<(String, String)>,
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
    /// The result with its list fields still empty; [`match_of`] fills them.
    result: MetadataResult,
    provenance: LookupProvenance,
    narrowed_out: bool,
}

/// The match its columns and its child rows describe. The media kind and the
/// medium rows have to agree: a kind with no entries is `undescribed`, a
/// `descriptors` row states its format, and a row of another kind is a
/// different match's.
pub(super) fn match_of(columns: MatchColumns, entries: MatchEntries) -> Result<MatchRow, DbError> {
    let MatchColumns {
        content_hash,
        position,
        pressing,
        media_kind,
        mut result,
        provenance,
        narrowed_out,
    } = columns;
    let MatchEntries {
        barcodes,
        media,
        links,
    } = entries;
    let mismatch = |what: &str| {
        DbError::Message(format!(
            "match {position} of {content_hash} holds {media_kind} media but {what}"
        ))
    };
    for (kind, _) in &media {
        if *kind != media_kind {
            return Err(mismatch(&format!("a {kind} medium row")));
        }
    }
    result.media = match media_kind.as_str() {
        MEDIA_UNDESCRIBED => {
            if !media.is_empty() {
                return Err(mismatch("lists medium rows"));
            }
            StatedMedia::Undescribed
        }
        MEDIA_PER_MEDIUM => {
            StatedMedia::PerMedium(media.into_iter().map(|(_, format)| format).collect())
        }
        MEDIA_DESCRIPTORS => StatedMedia::Descriptors(
            media
                .into_iter()
                .map(|(_, format)| format.ok_or_else(|| mismatch("a descriptor row states none")))
                .collect::<Result<_, _>>()?,
        ),
        other => return Err(unreadable("media_kind", other)),
    };
    result.barcodes = barcodes;
    result.links = links
        .into_iter()
        .map(|(catalog, key)| Ok(MetadataRef::new(source_of(&catalog)?, key)))
        .collect::<Result<_, DbError>>()?;
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
        (Some(url), Some(source)) => Some(RemoteCover {
            url,
            thumbnail_url: row.get("cover_thumbnail_url")?,
            label: row.get("cover_label")?,
            source: source_of(&source)?,
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
    Ok(MatchColumns {
        content_hash: row.get("content_hash")?,
        position: row.get("position")?,
        pressing,
        media_kind: row.get("media_kind")?,
        result: MetadataResult {
            source: source_of(&source)?,
            release_id: row.get("release_id")?,
            title: row.get("title")?,
            artist: row.get("artist")?,
            year: row.get("year")?,
            format: row.get("format")?,
            label: row.get("label")?,
            catalog_number: row.get("catalog_number")?,
            country: row.get("country")?,
            barcodes: Vec::new(),
            media: StatedMedia::Undescribed,
            links: Vec::new(),
            cover_art,
            source_group_id: row.get("source_group_id")?,
            source_tracks,
        },
        provenance: LookupProvenance {
            by_disc_id: row.get("by_disc_id")?,
            by_barcode: row.get("by_barcode")?,
            by_catalog: row.get("by_catalog")?,
            by_search: row.get("by_search")?,
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
}

pub(super) const VERDICT_COLUMNS: &str = "content_hash, kind, track_count, \
     failures_json, ledger_json, identified_at";

pub(super) fn read_verdict_row(row: &Row<'_>) -> Result<VerdictRow, DbError> {
    Ok(VerdictRow {
        content_hash: row.get("content_hash")?,
        kind: row.get("kind")?,
        track_count: row.get("track_count")?,
        failures_json: row.get("failures_json")?,
        ledger_json: row.get("ledger_json")?,
        identified_at: super::rfc3339_column(row, "identified_at")?,
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
    let verdict = match kind.as_str() {
        "found" => {
            let (matches, provenance, pressings) = unzip_stored(found.found);
            let (narrowed_out, narrowed_out_provenance, narrowed_out_pressings) =
                unzip_stored(found.narrowed_out);
            TerminalVerdict::Found {
                matches,
                track_count: count_of()?,
                provenance,
                pressings,
                narrowed_out,
                narrowed_out_provenance,
                narrowed_out_pressings,
                ledger,
            }
        }
        "not_found" => TerminalVerdict::NotFoundAnywhere { ledger },
        "manual_only" => TerminalVerdict::ManualOnly {
            track_count: count_of()?,
            ledger,
        },
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
