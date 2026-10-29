//! A candidate's settled signals, as one header row plus its list values and
//! text lines. How bae broke reading a signal is one column, its error chain.
//! A still-scanning signal reaching here is a defect, and the write refuses
//! it.

use super::verdict_rows::unreadable;
use super::*;
use crate::signals::{
    AudioOrigin, AudioSource, BarcodeSignal, CdProof, ComputedDiscId, DiscIdSignal, DownloadProof,
    InternalFailure, Signals, SourcedValue, StoreMarker, TextLine, TextSignal,
};

const SIGNALS_COLUMNS: &str = "content_hash, audio_source, cd_rip_proof, download_proof, \
     audio_source_file, not_cd_rate, \
     disc_id_state, \
     disc_id_failure, barcode_state, barcode_failure, text_state, text_failure";

/// The stored `download_proof` of a label's delivery set; a store's marker is
/// stored as its own key.
const DELIVERY_SET: &str = "delivery_set";

const SIGNAL_VALUE_COLUMNS: &str = "content_hash, list, position, value, origin_path";

const TEXT_LINE_COLUMNS: &str = "content_hash, position, text, origin";

/// One list's values under the header joined as `signals`, as a JSON array
/// in their order, each value as `value` gives it.
fn signal_list_sql(list: &str, value: &str) -> String {
    format!(
        "(SELECT json_group_array({value} ORDER BY listed.position) \
          FROM import_candidate_signal_value AS listed \
          WHERE listed.content_hash = signals.content_hash AND listed.list = '{list}')"
    )
}

/// A candidate's settled signals as columns of a read that joins their
/// header row as `signals`, each list and the text lines riding on it as a
/// JSON array; a Disc ID or barcode with the file it was read off. A
/// candidate with no header row reads `signals_row` as NULL.
pub(super) fn signals_read_columns() -> String {
    let sourced = "json_object('value', listed.value, 'origin_path', listed.origin_path)";
    format!(
        "signals.content_hash AS signals_row, signals.audio_source AS signals_audio_source, \
         signals.cd_rip_proof AS signals_cd_rip_proof, \
         signals.download_proof AS signals_download_proof, \
         signals.audio_source_file AS signals_audio_source_file, \
         signals.not_cd_rate AS signals_not_cd_rate, \
         signals.disc_id_state AS signals_disc_id_state, \
         signals.disc_id_failure AS signals_disc_id_failure, \
         signals.barcode_state AS signals_barcode_state, \
         signals.barcode_failure AS signals_barcode_failure, \
         signals.text_state AS signals_text_state, \
         signals.text_failure AS signals_text_failure, \
         {discs} AS signals_discs, {barcodes} AS signals_barcodes, \
         {catalogs} AS signals_catalogs, {free_text} AS signals_free_text, \
         {isrcs} AS signals_isrcs, {track_titles} AS signals_track_titles, \
         (SELECT json_group_array(json_object('text', line.text, 'origin', line.origin) \
                                  ORDER BY line.position) \
          FROM import_candidate_text_line AS line \
          WHERE line.content_hash = signals.content_hash) AS signals_text_pool",
        discs = signal_list_sql("disc_id", sourced),
        barcodes = signal_list_sql("barcode", sourced),
        catalogs = signal_list_sql("catalog", "listed.value"),
        free_text = signal_list_sql("free_text", "listed.value"),
        isrcs = signal_list_sql("isrc", "listed.value"),
        track_titles = signal_list_sql("track_title", "listed.value"),
    )
}

/// Every signal row under `content_hash`. The values cascade from the header.
pub(super) fn delete_signals(sql: &SqlContext<'_, '_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_signals WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

/// The settled signals as one header row and its list values, over whatever
/// signals stood under this hash: the header in place, its values anew.
pub(super) fn store_signals(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    signals: &Signals,
) -> Result<(), DbError> {
    let (disc_id_state, discs, disc_id_failure) = match &signals.disc_id {
        DiscIdSignal::Computed { discs } => ("computed", discs.as_slice(), None),
        DiscIdSignal::Absent => ("absent", [].as_slice(), None),
        DiscIdSignal::NotCdAudio => ("not_cd_audio", [].as_slice(), None),
        DiscIdSignal::Failed { failure } => ("failed", [].as_slice(), Some(failure)),
    };
    let (audio_source, cd_rip_proof, download_proof, audio_source_file) =
        match &signals.origin.source {
            Some(AudioSource::CdRip { proof, file }) => {
                (Some("cd_rip"), Some(proof.key()), None, file.clone())
            }
            Some(AudioSource::Download(DownloadProof::Store { marker, file })) => (
                Some("download"),
                None,
                Some(marker.key()),
                Some(file.clone()),
            ),
            Some(AudioSource::Download(DownloadProof::DeliverySet)) => {
                (Some("download"), None, Some(DELIVERY_SET), None)
            }
            None => (None, None, None, None),
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
    let detail = |failure: Option<&InternalFailure>| failure.map(|failure| failure.detail.clone());

    sql.execute(
        &format!(
            "INSERT INTO import_candidate_signals ({SIGNALS_COLUMNS}) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT (content_hash) DO UPDATE SET \
                 audio_source = excluded.audio_source, cd_rip_proof = excluded.cd_rip_proof, \
                 download_proof = excluded.download_proof, \
                 audio_source_file = excluded.audio_source_file, \
                 not_cd_rate = excluded.not_cd_rate, disc_id_state = excluded.disc_id_state, \
                 disc_id_failure = excluded.disc_id_failure, \
                 barcode_state = excluded.barcode_state, \
                 barcode_failure = excluded.barcode_failure, text_state = excluded.text_state, \
                 text_failure = excluded.text_failure"
        ),
        params![
            content_hash,
            audio_source,
            cd_rip_proof,
            download_proof,
            audio_source_file,
            signals.origin.not_cd_rate,
            disc_id_state,
            detail(disc_id_failure),
            barcode_state,
            detail(barcode_failure),
            text_state,
            detail(text_failure),
        ],
    )?;

    for table in [
        "import_candidate_signal_value",
        "import_candidate_text_line",
    ] {
        sql.execute(
            &format!("DELETE FROM {table} WHERE content_hash = ?"),
            [content_hash],
        )?;
    }
    let unsourced = |values: &[String]| -> Vec<(String, Option<String>)> {
        values.iter().map(|value| (value.clone(), None)).collect()
    };
    let lists = [
        (
            "disc_id",
            discs
                .iter()
                .map(|disc| (disc.disc_id.clone(), disc.source_file.clone()))
                .collect(),
        ),
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
        ("isrc", unsourced(&signals.isrcs)),
        ("track_title", unsourced(&signals.track_titles)),
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
            params![
                content_hash,
                position as i64,
                line.text,
                line.origin.as_str()
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

/// One candidate's [`signals_read_columns`] as SQLite hands them over, its
/// lists still JSON.
pub(super) struct StoredSignals {
    audio_source: Option<String>,
    cd_rip_proof: Option<String>,
    download_proof: Option<String>,
    audio_source_file: Option<String>,
    not_cd_rate: Option<u32>,
    disc_id_state: String,
    disc_id_failure: Option<String>,
    barcode_state: String,
    barcode_failure: Option<String>,
    text_state: String,
    text_failure: Option<String>,
    discs: String,
    barcodes: String,
    catalogs: String,
    free_text: String,
    isrcs: String,
    track_titles: String,
    text_pool: String,
}

/// The [`signals_read_columns`] of `row`; none for a candidate whose signals
/// were never settled.
pub(super) fn read_signals(row: &Row<'_>) -> rusqlite::Result<Option<StoredSignals>> {
    if row.get::<_, Option<String>>("signals_row")?.is_none() {
        return Ok(None);
    }
    Ok(Some(StoredSignals {
        audio_source: row.get("signals_audio_source")?,
        cd_rip_proof: row.get("signals_cd_rip_proof")?,
        download_proof: row.get("signals_download_proof")?,
        audio_source_file: row.get("signals_audio_source_file")?,
        not_cd_rate: row.get("signals_not_cd_rate")?,
        disc_id_state: row.get("signals_disc_id_state")?,
        disc_id_failure: row.get("signals_disc_id_failure")?,
        barcode_state: row.get("signals_barcode_state")?,
        barcode_failure: row.get("signals_barcode_failure")?,
        text_state: row.get("signals_text_state")?,
        text_failure: row.get("signals_text_failure")?,
        discs: row.get("signals_discs")?,
        barcodes: row.get("signals_barcodes")?,
        catalogs: row.get("signals_catalogs")?,
        free_text: row.get("signals_free_text")?,
        isrcs: row.get("signals_isrcs")?,
        track_titles: row.get("signals_track_titles")?,
        text_pool: row.get("signals_text_pool")?,
    }))
}

/// One sourced value of a list as [`signals_read_columns`] holds it.
#[derive(serde::Deserialize)]
struct StoredSourcedValue {
    value: String,
    origin_path: Option<String>,
}

/// One text line as [`signals_read_columns`] holds it.
#[derive(serde::Deserialize)]
struct StoredTextLine {
    text: String,
    origin: String,
}

fn list_of<T: serde::de::DeserializeOwned>(json: &str, what: &str) -> Result<Vec<T>, DbError> {
    serde_json::from_str(json)
        .map_err(|error| DbError::Message(format!("stored signal {what} {json:?}: {error}")))
}

impl StoredSignals {
    pub(super) fn signals(self) -> Result<Signals, DbError> {
        let Self {
            audio_source,
            cd_rip_proof,
            download_proof,
            audio_source_file,
            not_cd_rate,
            disc_id_state,
            disc_id_failure,
            barcode_state,
            barcode_failure,
            text_state,
            text_failure,
            discs,
            barcodes,
            catalogs,
            free_text,
            isrcs,
            track_titles,
            text_pool,
        } = self;
        let failure_of = |detail: Option<String>, what: &str| {
            detail
                .map(|detail| InternalFailure { detail })
                .ok_or_else(|| DbError::Message(format!("a failed {what} states no error")))
        };
        let source = match (audio_source.as_deref(), cd_rip_proof, download_proof) {
            (None, None, None) => None,
            (Some("cd_rip"), Some(proof), None) => Some(AudioSource::CdRip {
                proof: CdProof::from_key(&proof)
                    .ok_or_else(|| unreadable("cd_rip_proof", &proof))?,
                file: audio_source_file,
            }),
            (Some("download"), None, Some(proof)) if proof == DELIVERY_SET => {
                Some(AudioSource::Download(DownloadProof::DeliverySet))
            }
            (Some("download"), None, Some(proof)) => {
                let marker = StoreMarker::from_key(&proof)
                    .ok_or_else(|| unreadable("download_proof", &proof))?;
                let file = audio_source_file.ok_or_else(|| {
                    DbError::Message("a stored store download names no file".into())
                })?;
                Some(AudioSource::Download(DownloadProof::Store { marker, file }))
            }
            (source, ..) => return Err(unreadable("audio_source", source.unwrap_or("NULL"))),
        };
        let origin = AudioOrigin {
            source,
            not_cd_rate,
        };
        let discs: Vec<ComputedDiscId> = list_of::<StoredSourcedValue>(&discs, "Disc IDs")?
            .into_iter()
            .map(|disc| ComputedDiscId {
                disc_id: disc.value,
                source_file: disc.origin_path,
            })
            .collect();
        if (disc_id_state == "computed") == discs.is_empty() {
            return Err(DbError::Message(format!(
                "a {disc_id_state} Disc ID signal states {} Disc IDs",
                discs.len()
            )));
        }
        let disc_id = match disc_id_state.as_str() {
            "computed" => DiscIdSignal::Computed { discs },
            "absent" => DiscIdSignal::Absent,
            "not_cd_audio" => DiscIdSignal::NotCdAudio,
            "failed" => DiscIdSignal::Failed {
                failure: failure_of(disc_id_failure, "Disc ID")?,
            },
            other => return Err(unreadable("disc_id_state", other)),
        };
        let codes: Vec<SourcedValue> = list_of::<StoredSourcedValue>(&barcodes, "barcodes")?
            .into_iter()
            .map(|code| SourcedValue {
                value: code.value,
                origin_path: code.origin_path,
            })
            .collect();
        let barcode = match barcode_state.as_str() {
            "settled" => BarcodeSignal::Settled { codes },
            "absent" => BarcodeSignal::Absent,
            "failed" => BarcodeSignal::Failed {
                failure: failure_of(barcode_failure, "barcode")?,
                codes,
            },
            other => return Err(unreadable("barcode_state", other)),
        };
        let catalogs = list_of(&catalogs, "catalog numbers")?;
        let free_text = list_of(&free_text, "free text")?;
        let text = match text_state.as_str() {
            "settled" => TextSignal::Settled {
                catalogs,
                free_text,
            },
            "failed" => TextSignal::Failed {
                failure: failure_of(text_failure, "text signal")?,
                catalogs,
                free_text,
            },
            other => return Err(unreadable("text_state", other)),
        };
        let text_pool = list_of::<StoredTextLine>(&text_pool, "text lines")?
            .into_iter()
            .map(|line| {
                Ok(TextLine {
                    origin: line.origin.parse().map_err(DbError::Message)?,
                    text: line.text,
                })
            })
            .collect::<Result<_, DbError>>()?;
        Ok(Signals {
            origin,
            disc_id,
            barcode,
            text,
            text_pool,
            isrcs: list_of(&isrcs, "ISRCs")?,
            track_titles: list_of(&track_titles, "track titles")?,
        })
    }
}
