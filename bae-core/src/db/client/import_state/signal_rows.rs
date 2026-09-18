//! The settled signals of one candidate, as one header row plus its list
//! values.
//!
//! The three signal kinds each settle into a state plus, where they failed, a
//! typed [`LookupFailure`]. That failure is three columns — kind, the
//! provider's HTTP status, the diagnostic detail — and the same three columns
//! appear once per kind, so both directions go through one helper.
//!
//! Only a settled value is storable. `Scanning` is artwork OCR still running,
//! and a verdict is written only after the identify machine settled, which
//! waits for OCR — so a scanning signal reaching here is a defect and the
//! write says so rather than storing a half-read one.

use super::super::read::stored_region;
use super::verdict_rows::unreadable;
use super::*;
use crate::import::{TrackVerification, Verification, VerificationSource};
use crate::signals::{
    BarcodeSignal, DiscIdSignal, LookupFailure, SignalOrigin, Signals, SourcedValue, TextLine,
    TextSignal,
};

const SIGNALS_COLUMNS: &str = "content_hash, disc_id_state, disc_id, disc_id_source_file, \
     track_count, \
     disc_id_failure, disc_id_failure_status, disc_id_failure_detail, \
     barcode_state, barcode_failure, barcode_failure_status, barcode_failure_detail, \
     text_state, text_failure, text_failure_status, text_failure_detail, \
     verification_source";

const SIGNAL_VALUE_COLUMNS: &str = "content_hash, list, position, value, origin, origin_path, \
     region_x, region_y, region_width, region_height";

const TEXT_LINE_COLUMNS: &str = "content_hash, position, text, origin, origin_path, \
     region_x, region_y, region_width, region_height";

const VERIFICATION_COLUMNS: &str =
    "content_hash, track, accuraterip_confidence, ctdb_confidence, crc";

/// One failure as its three columns.
struct FailureColumns {
    kind: Option<&'static str>,
    status: Option<i64>,
    detail: Option<String>,
}

impl FailureColumns {
    const NONE: Self = Self {
        kind: None,
        status: None,
        detail: None,
    };
}

fn failure_columns(failure: Option<&LookupFailure>) -> FailureColumns {
    match failure {
        None => FailureColumns::NONE,
        Some(LookupFailure::Network) => FailureColumns {
            kind: Some("network"),
            ..FailureColumns::NONE
        },
        Some(LookupFailure::Timeout) => FailureColumns {
            kind: Some("timeout"),
            ..FailureColumns::NONE
        },
        Some(LookupFailure::ArtworkAnalysis) => FailureColumns {
            kind: Some("artwork_analysis"),
            ..FailureColumns::NONE
        },
        Some(LookupFailure::Provider { status }) => FailureColumns {
            kind: Some("provider"),
            status: status.map(i64::from),
            detail: None,
        },
        Some(LookupFailure::Diagnostic { detail }) => FailureColumns {
            kind: Some("diagnostic"),
            status: None,
            detail: Some(detail.clone()),
        },
    }
}

fn failure_of(
    kind: Option<String>,
    status: Option<i64>,
    detail: Option<String>,
) -> Result<Option<LookupFailure>, DbError> {
    let Some(kind) = kind else {
        return Ok(None);
    };
    Ok(Some(match kind.as_str() {
        "network" => LookupFailure::Network,
        "timeout" => LookupFailure::Timeout,
        "artwork_analysis" => LookupFailure::ArtworkAnalysis,
        "provider" => LookupFailure::Provider {
            status: status
                .map(|status| {
                    u16::try_from(status).map_err(|_| {
                        DbError::Message(format!("a stored provider status is {status}"))
                    })
                })
                .transpose()?,
        },
        "diagnostic" => LookupFailure::Diagnostic {
            detail: detail
                .ok_or_else(|| DbError::Message("a stored diagnostic states no detail".into()))?,
        },
        other => return Err(unreadable("signal failure", other)),
    }))
}

fn origin_of(stored: &str) -> Result<SignalOrigin, DbError> {
    stored.parse().map_err(DbError::Message)
}

/// Every signal row under `content_hash`. The values cascade from the header.
pub(super) fn delete_signals(sql: &SqlContext<'_, '_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_signals WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

/// The settled signals as one header row and its list values. The caller has
/// cleared what stood under this hash, so this writes into empty space.
pub(super) fn insert_signals(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    signals: &Signals,
) -> Result<(), DbError> {
    let (disc_id_state, disc_id, disc_id_source_file, disc_id_failure) = match &signals.disc_id {
        DiscIdSignal::Computed {
            disc_id,
            source_file,
            ..
        } => (
            "computed",
            Some(disc_id.as_str()),
            source_file.clone(),
            None,
        ),
        DiscIdSignal::Absent { .. } => ("absent", None, None, None),
        DiscIdSignal::Failed { failure, .. } => ("failed", None, None, Some(failure)),
    };
    let (barcode_state, barcode_failure) = match &signals.barcode {
        BarcodeSignal::Settled { .. } => ("settled", None),
        BarcodeSignal::Absent => ("absent", None),
        BarcodeSignal::Failed { failure, .. } => ("failed", Some(failure)),
        BarcodeSignal::Scanning { .. } => {
            return Err(DbError::Message(
                "signals still scanning at verdict write".to_string(),
            ))
        }
    };
    let (text_state, text_failure) = match &signals.text {
        TextSignal::Settled { .. } => ("settled", None),
        TextSignal::Failed { failure, .. } => ("failed", Some(failure)),
        TextSignal::Scanning { .. } => {
            return Err(DbError::Message(
                "signals still scanning at verdict write".to_string(),
            ))
        }
    };
    let disc_id_failure = failure_columns(disc_id_failure);
    let barcode_failure = failure_columns(barcode_failure);
    let text_failure = failure_columns(text_failure);

    sql.execute(
        &format!(
            "INSERT INTO import_candidate_signals ({SIGNALS_COLUMNS}) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ),
        params![
            content_hash,
            disc_id_state,
            disc_id,
            disc_id_source_file,
            signals.disc_id.track_count(),
            disc_id_failure.kind,
            disc_id_failure.status,
            disc_id_failure.detail,
            barcode_state,
            barcode_failure.kind,
            barcode_failure.status,
            barcode_failure.detail,
            text_state,
            text_failure.kind,
            text_failure.status,
            text_failure.detail,
            signals
                .verification
                .as_ref()
                .map(|verification| verification.source.as_str()),
        ],
    )?;

    // One row per track the log stated a result for. The header's source says
    // the candidate has a verification at all; these say what it is.
    for track in signals
        .verification
        .iter()
        .flat_map(|verification| &verification.tracks)
    {
        sql.execute(
            &format!(
                "INSERT INTO import_candidate_verification ({VERIFICATION_COLUMNS}) \
                 VALUES (?, ?, ?, ?, ?)"
            ),
            params![
                content_hash,
                track.number,
                track.accuraterip_confidence,
                track.ctdb_confidence,
                track.crc,
            ],
        )?;
    }

    let sourced = |list: &'static str, values: &[SourcedValue]| {
        values
            .iter()
            .enumerate()
            .map(|(position, value)| {
                (
                    list,
                    position as i64,
                    value.value.clone(),
                    Some(value.origin.as_str()),
                    value.origin_path.clone(),
                    value.region,
                )
            })
            .collect::<Vec<_>>()
    };
    let mut values = sourced("barcode", signals.barcode.codes());
    values.extend(sourced("catalog", signals.text.catalogs()));
    values.extend(
        free_text(&signals.text)
            .iter()
            .enumerate()
            .map(|(position, value)| {
                (
                    "free_text",
                    position as i64,
                    value.clone(),
                    None,
                    None,
                    None,
                )
            }),
    );
    for (list, position, value, origin, origin_path, region) in values {
        sql.execute(
            &format!(
                "INSERT INTO import_candidate_signal_value ({SIGNAL_VALUE_COLUMNS}) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            ),
            params![
                content_hash,
                list,
                position,
                value,
                origin,
                origin_path,
                region.map(|r| f64::from(r.x)),
                region.map(|r| f64::from(r.y)),
                region.map(|r| f64::from(r.width)),
                region.map(|r| f64::from(r.height)),
            ],
        )?;
    }

    // The candidate's own text, every line of it, in the order the pass read
    // it. Beside the values, not among them: nothing was extracted from these.
    for (position, line) in signals.text_pool.iter().enumerate() {
        sql.execute(
            &format!(
                "INSERT INTO import_candidate_text_line ({TEXT_LINE_COLUMNS}) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
            ),
            params![
                content_hash,
                position as i64,
                line.text,
                line.origin.as_str(),
                line.file,
                line.region.map(|r| f64::from(r.x)),
                line.region.map(|r| f64::from(r.y)),
                line.region.map(|r| f64::from(r.width)),
                line.region.map(|r| f64::from(r.height)),
            ],
        )?;
    }
    Ok(())
}

fn free_text(text: &TextSignal) -> &[String] {
    match text {
        TextSignal::Scanning { free_text, .. }
        | TextSignal::Settled { free_text, .. }
        | TextSignal::Failed { free_text, .. } => free_text,
    }
}

/// What a candidate's settled signals state about the object it was copied
/// from: the names its folder carries, and what the rip databases said about
/// its audio.
pub(crate) struct CandidateSignalFacts {
    /// One line per value, in `MarkKind` order.
    pub(crate) marks: Vec<crate::import::ReleaseMarkLine>,
    /// `None` for a candidate whose log states nothing about its bits.
    pub(crate) verification: Option<Verification>,
}

/// Those facts for every candidate, or for the one `only` names.
///
/// Read through the signals themselves rather than off the value rows
/// directly: which of a candidate's signals are names read off the object is
/// [`crate::import::ReleaseMark::of_signals`]'s answer, and asking it twice is
/// two answers to one question. That answer reads the person's lookup choices,
/// so they are loaded here beside the signals; a candidate with no stored
/// choices runs with the default, as every other reader of them does. Both
/// facts come out of one load, because both are readings of one settled
/// extraction.
pub(crate) fn load_signal_facts_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, CandidateSignalFacts>, DbError> {
    let signals = load_signals_on(sql, only)?;
    let choices = super::lookup_choice_rows::load_lookup_choices_on(sql, only)?;
    let mut choices = choices()?;
    Ok(signals()?
        .into_iter()
        .map(|(content_hash, signals)| {
            let choices = choices.remove(&content_hash).unwrap_or_default();
            let marks = crate::import::ReleaseMark::of_signals(&signals, &choices);
            (
                content_hash,
                CandidateSignalFacts {
                    marks: crate::import::ReleaseMarkLine::fold(&marks),
                    verification: signals.verification,
                },
            )
        })
        .collect())
}

/// Every candidate's settled signals, or the one `only` names.
pub(super) fn load_signals_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<impl FnOnce() -> Result<HashMap<String, Signals>, DbError> + Send + 'static, DbError> {
    let values = sql.query(
        &format!(
            "SELECT {SIGNAL_VALUE_COLUMNS} FROM import_candidate_signal_value \
             WHERE :only IS NULL OR content_hash = :only \
             ORDER BY content_hash, list, position"
        ),
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>("content_hash")?,
                row.get::<_, String>("list")?,
                row.get::<_, String>("value")?,
                row.get::<_, Option<String>>("origin")?,
                row.get::<_, Option<String>>("origin_path")?,
                [
                    row.get::<_, Option<f64>>("region_x")?,
                    row.get::<_, Option<f64>>("region_y")?,
                    row.get::<_, Option<f64>>("region_width")?,
                    row.get::<_, Option<f64>>("region_height")?,
                ],
            ))
        },
    )?;
    let text_lines = sql.query(
        &format!(
            "SELECT {TEXT_LINE_COLUMNS} FROM import_candidate_text_line \
             WHERE :only IS NULL OR content_hash = :only \
             ORDER BY content_hash, position"
        ),
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>("content_hash")?,
                row.get::<_, String>("text")?,
                row.get::<_, String>("origin")?,
                row.get::<_, Option<String>>("origin_path")?,
                [
                    row.get::<_, Option<f64>>("region_x")?,
                    row.get::<_, Option<f64>>("region_y")?,
                    row.get::<_, Option<f64>>("region_width")?,
                    row.get::<_, Option<f64>>("region_height")?,
                ],
            ))
        },
    )?;
    let verifications = sql.query(
        &format!(
            "SELECT {VERIFICATION_COLUMNS} FROM import_candidate_verification \
             WHERE :only IS NULL OR content_hash = :only \
             ORDER BY content_hash, track"
        ),
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>("content_hash")?,
                row.get::<_, i64>("track")?,
                row.get::<_, Option<i64>>("accuraterip_confidence")?,
                row.get::<_, Option<i64>>("ctdb_confidence")?,
                row.get::<_, Option<i64>>("crc")?,
            ))
        },
    )?;
    let rows = sql.query(
        &format!(
            "SELECT {SIGNALS_COLUMNS} FROM import_candidate_signals \
             WHERE :only IS NULL OR content_hash = :only"
        ),
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>("content_hash")?,
                row.get::<_, String>("disc_id_state")?,
                row.get::<_, Option<String>>("disc_id")?,
                row.get::<_, Option<String>>("disc_id_source_file")?,
                row.get::<_, i64>("track_count")?,
                row.get::<_, Option<String>>("disc_id_failure")?,
                row.get::<_, Option<i64>>("disc_id_failure_status")?,
                row.get::<_, Option<String>>("disc_id_failure_detail")?,
                row.get::<_, String>("barcode_state")?,
                row.get::<_, Option<String>>("barcode_failure")?,
                row.get::<_, Option<i64>>("barcode_failure_status")?,
                row.get::<_, Option<String>>("barcode_failure_detail")?,
                row.get::<_, String>("text_state")?,
                row.get::<_, Option<String>>("text_failure")?,
                row.get::<_, Option<i64>>("text_failure_status")?,
                row.get::<_, Option<String>>("text_failure_detail")?,
                row.get::<_, Option<String>>("verification_source")?,
            ))
        },
    )?;

    Ok(move || {
        let mut lists: HashMap<String, SignalValues> = HashMap::new();
        for (content_hash, list, value, origin, origin_path, region) in values {
            let entry = lists.entry(content_hash).or_default();
            match list.as_str() {
                "barcode" => {
                    entry
                        .barcodes
                        .push(sourced_value(value, origin, origin_path, region)?)
                }
                "catalog" => {
                    entry
                        .catalogs
                        .push(sourced_value(value, origin, origin_path, region)?)
                }
                "free_text" => entry.free_text.push(value),
                other => return Err(unreadable("list", other)),
            }
        }
        let mut verified: HashMap<String, Vec<TrackVerification>> = HashMap::new();
        for (content_hash, track, accuraterip_confidence, ctdb_confidence, crc) in verifications {
            verified
                .entry(content_hash)
                .or_default()
                .push(TrackVerification {
                    number: stored_count("track", track)?,
                    accuraterip_confidence: accuraterip_confidence
                        .map(|value| stored_count("accuraterip confidence", value))
                        .transpose()?,
                    ctdb_confidence: ctdb_confidence
                        .map(|value| stored_count("CTDB confidence", value))
                        .transpose()?,
                    crc: crc.map(|value| stored_count("CRC", value)).transpose()?,
                });
        }
        let mut pools: HashMap<String, Vec<TextLine>> = HashMap::new();
        for (content_hash, text, origin, origin_path, region) in text_lines {
            pools.entry(content_hash).or_default().push(TextLine {
                region: stored_region(&text, region)?,
                origin: origin_of(&origin)?,
                file: origin_path,
                text,
            });
        }

        let mut out = HashMap::with_capacity(rows.len());
        for row in rows {
            let (
                content_hash,
                disc_id_state,
                disc_id,
                disc_id_source_file,
                track_count,
                disc_id_failure,
                disc_id_failure_status,
                disc_id_failure_detail,
                barcode_state,
                barcode_failure,
                barcode_failure_status,
                barcode_failure_detail,
                text_state,
                text_failure,
                text_failure_status,
                text_failure_detail,
                verification_source,
            ) = row;
            let values = lists.remove(&content_hash).unwrap_or_default();
            let track_count = u32::try_from(track_count).map_err(|_| {
                DbError::Message(format!("a stored signal counts {track_count} tracks"))
            })?;
            let disc_id = match disc_id_state.as_str() {
                "computed" => DiscIdSignal::Computed {
                    disc_id: disc_id.ok_or_else(|| {
                        DbError::Message("a computed disc ID signal states no hash".into())
                    })?,
                    track_count,
                    source_file: disc_id_source_file,
                },
                "absent" => DiscIdSignal::Absent { track_count },
                "failed" => DiscIdSignal::Failed {
                    failure: failure_of(
                        disc_id_failure,
                        disc_id_failure_status,
                        disc_id_failure_detail,
                    )?
                    .ok_or_else(|| DbError::Message("a failed disc ID states no reason".into()))?,
                    track_count,
                },
                other => return Err(unreadable("disc_id_state", other)),
            };
            let barcode = match barcode_state.as_str() {
                "settled" => BarcodeSignal::Settled {
                    codes: values.barcodes,
                },
                "absent" => BarcodeSignal::Absent,
                "failed" => BarcodeSignal::Failed {
                    failure: failure_of(
                        barcode_failure,
                        barcode_failure_status,
                        barcode_failure_detail,
                    )?
                    .ok_or_else(|| DbError::Message("a failed barcode states no reason".into()))?,
                    codes: values.barcodes,
                },
                other => return Err(unreadable("barcode_state", other)),
            };
            let text = match text_state.as_str() {
                "settled" => TextSignal::Settled {
                    catalogs: values.catalogs,
                    free_text: values.free_text,
                },
                "failed" => TextSignal::Failed {
                    failure: failure_of(text_failure, text_failure_status, text_failure_detail)?
                        .ok_or_else(|| DbError::Message("failed text states no reason".into()))?,
                    catalogs: values.catalogs,
                    free_text: values.free_text,
                },
                other => return Err(unreadable("text_state", other)),
            };
            let text_pool = pools.remove(&content_hash).unwrap_or_default();
            let tracks = verified.remove(&content_hash).unwrap_or_default();
            // A source with no track rows under it, or rows with no source
            // over them, is half a verification — one write put both there.
            let verification = match (verification_source, tracks.is_empty()) {
                (None, true) => None,
                (Some(source), false) => Some(Verification {
                    source: source
                        .parse::<VerificationSource>()
                        .map_err(DbError::Message)?,
                    tracks,
                }),
                (source, _) => {
                    return Err(DbError::Message(format!(
                        "candidate {content_hash} states verification source {source:?} \
                         with no track rows, or track rows with no source"
                    )))
                }
            };
            out.insert(
                content_hash,
                Signals {
                    disc_id,
                    verification,
                    barcode,
                    text,
                    text_pool,
                    durations: Default::default(),
                },
            );
        }
        Ok(out)
    })
}

#[derive(Default)]
struct SignalValues {
    barcodes: Vec<SourcedValue>,
    catalogs: Vec<SourcedValue>,
    free_text: Vec<String>,
}

/// A stored count back as the `u32` the type holds it as. Every one of these
/// columns is written from a `u32`, so a value outside that range is a row
/// nothing here wrote.
fn stored_count(what: &str, value: i64) -> Result<u32, DbError> {
    u32::try_from(value)
        .map_err(|_| DbError::Message(format!("a stored {what} is {value}")))
}

fn sourced_value(
    value: String,
    origin: Option<String>,
    origin_path: Option<String>,
    region: [Option<f64>; 4],
) -> Result<SourcedValue, DbError> {
    let origin = origin
        .ok_or_else(|| DbError::Message(format!("the stored value {value:?} states no origin")))?;
    let origin = origin_of(&origin)?;
    let region = stored_region(&value, region)?;
    Ok(match origin_path {
        Some(file_id) => SourcedValue::in_file(value, origin, file_id),
        None => SourcedValue::new(value, origin),
    }
    .at(region))
}

