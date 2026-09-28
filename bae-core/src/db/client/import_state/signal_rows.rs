//! A candidate's settled signals, as one header row plus its list values and
//! text lines. Each signal's [`crate::signals::LookupFailure`] is three columns (kind, status,
//! detail). A still-scanning signal reaching here is a defect, and the write
//! refuses it.

use super::lookup_failure_columns::{failure_columns, failure_of};
use super::verdict_rows::unreadable;
use super::*;
use crate::signals::{
    BarcodeSignal, CdProof, DiscIdSignal, RipEvidence, Signals, SourcedValue,
    TextLine, TextSignal,
};

const SIGNALS_COLUMNS: &str = "content_hash, rip, rip_proof, rip_file, \
     disc_id_state, disc_id, disc_id_source_file, \
     disc_id_failure, disc_id_failure_status, disc_id_failure_detail, \
     barcode_state, barcode_failure, barcode_failure_status, barcode_failure_detail, \
     text_state, text_failure, text_failure_status, text_failure_detail, \
     registered_in_country, registered_in_region";

const SIGNAL_VALUE_COLUMNS: &str = "content_hash, list, position, value, origin_path";

const TEXT_LINE_COLUMNS: &str = "content_hash, position, text, origin";

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
        DiscIdSignal::Absent => ("absent", None, None, None),
        DiscIdSignal::NotCdAudio => ("not_cd_audio", None, None, None),
        DiscIdSignal::Failed { failure } => ("failed", None, None, Some(failure)),
    };
    let (rip, rip_proof, rip_file) = match &signals.rip {
        RipEvidence::Cd { proof, file } => ("cd", Some(proof.key()), file.clone()),
        RipEvidence::NotCd => ("not_cd", None, None),
        RipEvidence::Unproven => ("unproven", None, None),
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
    let (registered_in_country, registered_in_region) =
        crate::db::client::pressing_columns::area_columns(signals.registered_in);
    let disc_id_failure = failure_columns(disc_id_failure);
    let barcode_failure = failure_columns(barcode_failure);
    let text_failure = failure_columns(text_failure);

    sql.execute(
        &format!(
            "INSERT INTO import_candidate_signals ({SIGNALS_COLUMNS}) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ),
        params![
            content_hash,
            rip,
            rip_proof,
            rip_file,
            disc_id_state,
            disc_id,
            disc_id_source_file,
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
            registered_in_country,
            registered_in_region,
        ],
    )?;

    let unsourced = |values: &[String]| -> Vec<(String, Option<String>)> {
        values.iter().map(|value| (value.clone(), None)).collect()
    };
    let lists = [
        (
            "barcode",
            signals
                .barcode
                .codes()
                .iter()
                .map(|code| (code.value.clone(), code.origin_path.clone()))
                .collect(),
        ),
        ("catalog", unsourced(signals.text.catalogs())),
        ("free_text", unsourced(free_text(&signals.text))),
    ];
    for (list, values) in lists {
        for (position, (value, origin_path)) in values.into_iter().enumerate() {
            sql.execute(
                &format!(
                    "INSERT INTO import_candidate_signal_value ({SIGNAL_VALUE_COLUMNS}) \
                     VALUES (?, ?, ?, ?, ?)"
                ),
                params![content_hash, list, position as i64, value, origin_path],
            )?;
        }
    }

    // The candidate's own text, in reading order.
    for (position, line) in signals.text_pool.iter().enumerate() {
        sql.execute(
            &format!(
                "INSERT INTO import_candidate_text_line ({TEXT_LINE_COLUMNS}) \
                 VALUES (?, ?, ?, ?)"
            ),
            params![content_hash, position as i64, line.text, line.origin.as_str()],
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
                row.get::<_, Option<String>>("origin_path")?,
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
                (
                    row.get::<_, String>("rip")?,
                    row.get::<_, Option<String>>("rip_proof")?,
                    row.get::<_, Option<String>>("rip_file")?,
                ),
                row.get::<_, String>("disc_id_state")?,
                row.get::<_, Option<String>>("disc_id")?,
                row.get::<_, Option<String>>("disc_id_source_file")?,
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
                (
                    row.get::<_, Option<String>>("registered_in_country")?,
                    row.get::<_, Option<String>>("registered_in_region")?,
                ),
            ))
        },
    )?;

    Ok(move || {
        let mut lists: HashMap<String, SignalValues> = HashMap::new();
        for (content_hash, list, value, origin_path) in values {
            let entry = lists.entry(content_hash).or_default();
            match list.as_str() {
                "barcode" => entry.barcodes.push(SourcedValue { value, origin_path }),
                "catalog" => entry.catalogs.push(value),
                "free_text" => entry.free_text.push(value),
                other => return Err(unreadable("list", other)),
            }
        }
        let mut pools: HashMap<String, Vec<TextLine>> = HashMap::new();
        for (content_hash, text, origin) in text_lines {
            pools.entry(content_hash).or_default().push(TextLine {
                origin: origin.parse().map_err(DbError::Message)?,
                text,
            });
        }

        let mut out = HashMap::with_capacity(rows.len());
        for row in rows {
            let (
                content_hash,
                (rip, rip_proof, rip_file),
                disc_id_state,
                disc_id,
                disc_id_source_file,
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
                (registered_in_country, registered_in_region),
            ) = row;
            let registered_in = crate::db::client::pressing_columns::area_of(
                registered_in_country,
                registered_in_region,
            )
            .map_err(|detail| DbError::Message(format!("a stored registration: {detail}")))?;
            let values = lists.remove(&content_hash).unwrap_or_default();
            let rip = match rip.as_str() {
                "cd" => {
                    let proof = rip_proof.ok_or_else(|| {
                        DbError::Message("a stored CD rip states no proof".into())
                    })?;
                    RipEvidence::Cd {
                        proof: CdProof::from_key(&proof)
                            .ok_or_else(|| unreadable("rip_proof", &proof))?,
                        file: rip_file,
                    }
                }
                "not_cd" => RipEvidence::NotCd,
                "unproven" => RipEvidence::Unproven,
                other => return Err(unreadable("rip", other)),
            };
            let disc_id = match disc_id_state.as_str() {
                "computed" => DiscIdSignal::Computed {
                    disc_id: disc_id.ok_or_else(|| {
                        DbError::Message("a computed disc ID signal states no hash".into())
                    })?,
                    source_file: disc_id_source_file,
                },
                "absent" => DiscIdSignal::Absent,
                "not_cd_audio" => DiscIdSignal::NotCdAudio,
                "failed" => DiscIdSignal::Failed {
                    failure: failure_of(
                        disc_id_failure,
                        disc_id_failure_status,
                        disc_id_failure_detail,
                    )?
                    .ok_or_else(|| DbError::Message("a failed disc ID states no reason".into()))?,
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
            out.insert(
                content_hash,
                Signals {
                    rip,
                    disc_id,
                    barcode,
                    text,
                    text_pool,
                    registered_in,
                },
            );
        }
        Ok(out)
    })
}

#[derive(Default)]
struct SignalValues {
    barcodes: Vec<SourcedValue>,
    catalogs: Vec<String>,
    free_text: Vec<String>,
}
