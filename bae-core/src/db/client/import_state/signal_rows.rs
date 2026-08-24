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

use super::verdict_rows::unreadable;
use super::*;
use crate::signals::{
    BarcodeSignal, DiscIdSignal, LookupFailure, SignalOrigin, Signals, SourcedValue, TextSignal,
};

const SIGNALS_COLUMNS: &str = "content_hash, disc_id_state, disc_id, track_count, \
     disc_id_failure, disc_id_failure_status, disc_id_failure_detail, \
     barcode_state, barcode_failure, barcode_failure_status, barcode_failure_detail, \
     text_state, text_failure, text_failure_status, text_failure_detail";

const SIGNAL_VALUE_COLUMNS: &str = "content_hash, list, position, value, origin, origin_path";

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

fn origin_str(origin: SignalOrigin) -> &'static str {
    match origin {
        SignalOrigin::DiscToc => "disc_toc",
        SignalOrigin::CueSheet => "cue_sheet",
        SignalOrigin::Artwork => "artwork",
        SignalOrigin::FolderName => "folder_name",
        SignalOrigin::Filename => "filename",
        SignalOrigin::TextFile => "text_file",
    }
}

fn origin_of(stored: &str) -> Result<SignalOrigin, DbError> {
    Ok(match stored {
        "disc_toc" => SignalOrigin::DiscToc,
        "cue_sheet" => SignalOrigin::CueSheet,
        "artwork" => SignalOrigin::Artwork,
        "folder_name" => SignalOrigin::FolderName,
        "filename" => SignalOrigin::Filename,
        "text_file" => SignalOrigin::TextFile,
        other => return Err(unreadable("origin", other)),
    })
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
    let (disc_id_state, disc_id, disc_id_failure) = match &signals.disc_id {
        DiscIdSignal::Computed { disc_id, .. } => ("computed", Some(disc_id.as_str()), None),
        DiscIdSignal::Absent { .. } => ("absent", None, None),
        DiscIdSignal::Failed { failure, .. } => ("failed", None, Some(failure)),
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
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ),
        params![
            content_hash,
            disc_id_state,
            disc_id,
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
        ],
    )?;

    let sourced = |list: &'static str, values: &[SourcedValue]| {
        values
            .iter()
            .enumerate()
            .map(|(position, value)| {
                (
                    list,
                    position as i64,
                    value.value.clone(),
                    Some(origin_str(value.origin)),
                    value.origin_path.clone(),
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
            .map(|(position, value)| ("free_text", position as i64, value.clone(), None, None)),
    );
    for (list, position, value, origin, origin_path) in values {
        sql.execute(
            &format!(
                "INSERT INTO import_candidate_signal_value ({SIGNAL_VALUE_COLUMNS}) \
                 VALUES (?, ?, ?, ?, ?, ?)"
            ),
            params![content_hash, list, position, value, origin, origin_path],
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

/// Every candidate's settled signals, or the one `only` names.
///
/// `durations` comes from the duration rows rather than this table: the total
/// is derived from them, so storing it twice would let the two disagree.
pub(super) fn load_signals_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
    durations: &HashMap<String, crate::import::probe::ProbedDurations>,
) -> Result<HashMap<String, Signals>, DbError> {
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
            ))
        },
    )?;
    let mut lists: HashMap<String, SignalValues> = HashMap::new();
    for (content_hash, list, value, origin, origin_path) in values {
        let entry = lists.entry(content_hash).or_default();
        match list.as_str() {
            "barcode" => entry
                .barcodes
                .push(sourced_value(value, origin, origin_path)?),
            "catalog" => entry
                .catalogs
                .push(sourced_value(value, origin, origin_path)?),
            "free_text" => entry.free_text.push(value),
            other => return Err(unreadable("list", other)),
        }
    }

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
            ))
        },
    )?;

    let mut out = HashMap::with_capacity(rows.len());
    for row in rows {
        let (
            content_hash,
            disc_id_state,
            disc_id,
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
        let durations = durations.get(&content_hash).cloned().unwrap_or_default();
        out.insert(
            content_hash,
            Signals {
                disc_id,
                barcode,
                text,
                durations,
            },
        );
    }
    Ok(out)
}

#[derive(Default)]
struct SignalValues {
    barcodes: Vec<SourcedValue>,
    catalogs: Vec<SourcedValue>,
    free_text: Vec<String>,
}

fn sourced_value(
    value: String,
    origin: Option<String>,
    origin_path: Option<String>,
) -> Result<SourcedValue, DbError> {
    let origin = origin
        .ok_or_else(|| DbError::Message(format!("the stored value {value:?} states no origin")))?;
    let origin = origin_of(&origin)?;
    Ok(match origin_path {
        Some(file_id) => SourcedValue::in_file(value, origin, file_id),
        None => SourcedValue::new(value, origin),
    })
}
