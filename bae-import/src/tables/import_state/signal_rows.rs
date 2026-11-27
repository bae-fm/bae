//! A candidate's settled signals, as one header row plus its list values and
//! text lines; or the signals a run in flight last read, under the run. How
//! bae broke reading a signal is one column, its error chain. A still-scanning
//! signal reaching a candidate's rows is a defect, and the write refuses it.
//! Its tables are `signal_rows.sql`, which every device's library holds.

use super::verdict_rows::Owner;
use super::*;
use bae_catalogs::{
    AudioOrigin, AudioSource, CdProof, DownloadProof, InternalFailure, StoreMarker,
};
use bae_identify::signals::{
    BarcodeSignal, ComputedDiscId, DiscIdSignal, PaneSignals, Signals, SourcedValue, TextLine,
    TextSignal,
};

const SIGNALS_COLUMNS: &str =
    "content_hash, identify_run, audio_source, cd_rip_proof, download_proof, \
     audio_source_file, not_cd_rate, \
     disc_id_state, \
     disc_id_failure, barcode_state, barcode_failure, text_state, text_failure";

/// The stored `download_proof` of a label's delivery set; a store's marker is
/// stored as its own key.
const DELIVERY_SET: &str = "delivery_set";

const SIGNAL_VALUE_COLUMNS: &str = "signals_id, list, position, value, origin_path";

const TEXT_LINE_COLUMNS: &str = "signals_id, position, text, origin";

/// The id of an `import_candidate_signals` header row, which every value and
/// text line under it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SignalsId(i64);

impl rusqlite::ToSql for SignalsId {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

/// The Disc IDs, then the barcodes, of the release the scan stores at the
/// key a statement binds as `:key`, that name the candidate file they were
/// read off, each list in reading order, while the candidate's state is the
/// current one, as the JSON array that [`super::fragments::FileEvidence`]
/// returns. A library release's values name no file and are not selected.
pub(crate) fn file_evidence_of_json(json: &str) -> Result<Vec<crate::FileEvidence>, DbError> {
    #[derive(serde::Deserialize)]
    struct Stored {
        list: String,
        value: String,
        origin_path: String,
    }
    let stored: Vec<Stored> = serde_json::from_str(json).map_err(|error| {
        DbError::unreadable_json("the evidence in a candidate's files", json, error)
    })?;
    stored
        .into_iter()
        .map(|Stored { list, value, origin_path: file_id }| {
            let signal = match list.as_str() {
                "disc_id" => crate::EvidenceSignal::DiscId,
                "barcode" => crate::EvidenceSignal::Barcode,
                other => {
                    return Err(DbError::unreadable(
                        "import_candidate_signal_value.list of file evidence",
                        other,
                    ))
                }
            };
            Ok(crate::FileEvidence { signal, value, file_id })
        })
        .collect()
}

/// Every signal row under `content_hash`. The values cascade from the header.
pub(super) fn delete_signals(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_candidate_signals WHERE content_hash = ?", [content_hash])?;
    Ok(())
}

/// The signals as one header row and its list values, over whatever signals
/// `owner` held: the header in place, its values anew. Only a run in flight
/// stores a pass still scanning.
pub(super) fn store_signals(
    sql: &CovenWrite<'_>,
    owner: Owner<'_>,
    signals: &Signals,
) -> Result<(), DbError> {
    // A verdict is written once extraction stopped scanning.
    let scanning_refused = || -> ! { panic!("signals of {owner} still scanning at verdict write") };
    let running = matches!(owner, Owner::Run(_));
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
            Some(AudioSource::Download(DownloadProof::Store { marker, file })) => {
                (Some("download"), None, Some(marker.key()), Some(file.clone()))
            }
            Some(AudioSource::Download(DownloadProof::DeliverySet)) => {
                (Some("download"), None, Some(DELIVERY_SET), None)
            }
            None => (None, None, None, None),
        };
    let (barcode_state, barcode_failure) = match &signals.barcode {
        BarcodeSignal::Settled { .. } => ("settled", None),
        BarcodeSignal::Absent => ("absent", None),
        BarcodeSignal::Failed { failure, .. } => ("failed", Some(failure)),
        BarcodeSignal::Scanning { .. } if running => ("scanning", None),
        BarcodeSignal::Scanning { .. } => scanning_refused(),
    };
    let (text_state, text_failure) = match &signals.text {
        TextSignal::Settled { .. } => ("settled", None),
        TextSignal::Failed { failure, .. } => ("failed", Some(failure)),
        TextSignal::Scanning { .. } if running => ("scanning", None),
        TextSignal::Scanning { .. } => scanning_refused(),
    };
    let detail = |failure: Option<&InternalFailure>| failure.map(|failure| failure.detail.clone());

    let signals_id = SignalsId(sql.query_row(
        &format!(
            "INSERT INTO import_candidate_signals ({SIGNALS_COLUMNS}) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT ({column}) DO UPDATE SET \
                 audio_source = excluded.audio_source, cd_rip_proof = excluded.cd_rip_proof, \
                 download_proof = excluded.download_proof, \
                 audio_source_file = excluded.audio_source_file, \
                 not_cd_rate = excluded.not_cd_rate, disc_id_state = excluded.disc_id_state, \
                 disc_id_failure = excluded.disc_id_failure, \
                 barcode_state = excluded.barcode_state, \
                 barcode_failure = excluded.barcode_failure, text_state = excluded.text_state, \
                 text_failure = excluded.text_failure \
             RETURNING signals_id",
            column = owner.column(),
        ),
        params![
            owner.content_hash(),
            owner.run(),
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
        |row| row.get(0),
    )?);

    for table in ["import_candidate_signal_value", "import_candidate_text_line"] {
        sql.execute(&format!("DELETE FROM {table} WHERE signals_id = ?"), params![signals_id])?;
    }
    let unsourced = |values: &[String]| -> Vec<(String, Option<String>)> {
        values.iter().map(|value| (value.clone(), None)).collect()
    };
    let lists = [
        (
            "disc_id",
            discs.iter().map(|disc| (disc.disc_id.clone(), disc.source_file.clone())).collect(),
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
                params![signals_id, list, position as i64, value, origin_path],
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
            params![signals_id, position as i64, line.text, line.origin.as_str()],
        )?;
    }
    // The text changed, so check and group the verdict's rows again against
    // it.
    super::verdict_layout::lay_out_matches_on(sql, owner)?;
    Ok(())
}

fn free_text(text: &TextSignal) -> &[String] {
    match text {
        TextSignal::Scanning { free_text, .. } | TextSignal::Settled { free_text, .. } => free_text,
        TextSignal::Failed { .. } => &[],
    }
}

/// One candidate's signals but the text lines, as the signals fragment's
/// columns hold them, its lists still JSON.
pub(super) struct StoredPaneSignals {
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
}

/// One candidate's signals as the fragments hold them: the columns a pane
/// reads, and the text lines, still JSON.
pub(super) struct StoredSignals {
    pane: StoredPaneSignals,
    text_pool: String,
}

/// One sourced value of a list as the signals' list holds it.
#[derive(serde::Deserialize)]
struct StoredSourcedValue {
    value: String,
    origin_path: Option<String>,
}

/// One text line as the signals' text pool holds it.
#[derive(serde::Deserialize)]
struct StoredTextLine {
    text: String,
    origin: String,
}

fn list_of<T: serde::de::DeserializeOwned>(json: &str, what: &str) -> Result<Vec<T>, DbError> {
    serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json(format!("stored signal {what}"), json, error))
}

impl StoredPaneSignals {
    /// The signals a pane's read selects, as
    /// [`super::state_fragments::Signals`] names them; none for a candidate
    /// whose signals were never settled.
    pub(super) fn of(signals: Option<&super::state_fragments::Signals>) -> Option<Self> {
        let signals = signals?;
        Some(Self {
            audio_source: signals.signals_audio_source.clone(),
            cd_rip_proof: signals.signals_cd_rip_proof.clone(),
            download_proof: signals.signals_download_proof.clone(),
            audio_source_file: signals.signals_audio_source_file.clone(),
            not_cd_rate: signals.signals_not_cd_rate,
            disc_id_state: signals.signals_disc_id_state.clone(),
            disc_id_failure: signals.signals_disc_id_failure.clone(),
            barcode_state: signals.signals_barcode_state.clone(),
            barcode_failure: signals.signals_barcode_failure.clone(),
            text_state: signals.signals_text_state.clone(),
            text_failure: signals.signals_text_failure.clone(),
            discs: signals.signals_discs.clone(),
            barcodes: signals.signals_barcodes.clone(),
            catalogs: signals.signals_catalogs.clone(),
            free_text: signals.signals_free_text.clone(),
            isrcs: signals.signals_isrcs.clone(),
            track_titles: signals.signals_track_titles.clone(),
        })
    }

    pub(super) fn signals(self) -> Result<PaneSignals, DbError> {
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
        } = self;
        let failure_of = |detail: Option<String>, what: &str| {
            detail.map(|detail| InternalFailure { detail }).ok_or_else(|| {
                DbError::unreadable(
                    format!("the error of a failed {what} in import_candidate_signals"),
                    None::<String>,
                )
            })
        };
        let source = match (audio_source.as_deref(), cd_rip_proof, download_proof) {
            (None, None, None) => None,
            (Some("cd_rip"), Some(proof), None) => Some(AudioSource::CdRip {
                proof: CdProof::from_key(&proof).ok_or_else(|| {
                    DbError::unreadable("import_candidate_signals.cd_rip_proof", &proof)
                })?,
                file: audio_source_file,
            }),
            (Some("download"), None, Some(proof)) if proof == DELIVERY_SET => {
                Some(AudioSource::Download(DownloadProof::DeliverySet))
            }
            (Some("download"), None, Some(proof)) => {
                let marker = StoreMarker::from_key(&proof).ok_or_else(|| {
                    DbError::unreadable("import_candidate_signals.download_proof", &proof)
                })?;
                let file = audio_source_file.ok_or_else(|| {
                    DbError::unreadable(
                        "import_candidate_signals.audio_source_file of a store download",
                        None::<String>,
                    )
                })?;
                Some(AudioSource::Download(DownloadProof::Store { marker, file }))
            }
            (source, ..) => {
                return Err(DbError::unreadable(
                    "import_candidate_signals.audio_source",
                    source.unwrap_or("NULL"),
                ))
            }
        };
        let origin = AudioOrigin { source, not_cd_rate };
        let discs: Vec<ComputedDiscId> = list_of::<StoredSourcedValue>(&discs, "Disc IDs")?
            .into_iter()
            .map(|disc| ComputedDiscId { disc_id: disc.value, source_file: disc.origin_path })
            .collect();
        if (disc_id_state == "computed") == discs.is_empty() {
            return Err(DbError::unreadable(
                format!("the Disc IDs of a {disc_id_state} Disc ID signal"),
                discs.len(),
            ));
        }
        let disc_id = match disc_id_state.as_str() {
            "computed" => DiscIdSignal::Computed { discs },
            "absent" => DiscIdSignal::Absent,
            "not_cd_audio" => DiscIdSignal::NotCdAudio,
            "failed" => DiscIdSignal::Failed { failure: failure_of(disc_id_failure, "Disc ID")? },
            other => {
                return Err(DbError::unreadable("import_candidate_signals.disc_id_state", other))
            }
        };
        let codes: Vec<SourcedValue> = list_of::<StoredSourcedValue>(&barcodes, "barcodes")?
            .into_iter()
            .map(|code| SourcedValue { value: code.value, origin_path: code.origin_path })
            .collect();
        let barcode = match barcode_state.as_str() {
            "scanning" => BarcodeSignal::Scanning { codes },
            "settled" => BarcodeSignal::Settled { codes },
            "absent" => BarcodeSignal::Absent,
            "failed" => BarcodeSignal::Failed { failure: failure_of(barcode_failure, "barcode")? },
            other => {
                return Err(DbError::unreadable("import_candidate_signals.barcode_state", other))
            }
        };
        let catalogs = list_of(&catalogs, "catalog numbers")?;
        let free_text = list_of(&free_text, "free text")?;
        let text = match text_state.as_str() {
            "scanning" => TextSignal::Scanning { catalogs, free_text },
            "settled" => TextSignal::Settled { catalogs, free_text },
            "failed" => TextSignal::Failed { failure: failure_of(text_failure, "text signal")? },
            other => return Err(DbError::unreadable("import_candidate_signals.text_state", other)),
        };
        Ok(PaneSignals {
            origin,
            disc_id,
            barcode,
            text,
            isrcs: list_of(&isrcs, "ISRCs")?,
            track_titles: list_of(&track_titles, "track titles")?,
        })
    }
}

impl StoredSignals {
    /// The signals of a pane's columns with the text lines' JSON.
    pub(super) fn of(pane: StoredPaneSignals, text_pool: String) -> Self {
        Self { pane, text_pool }
    }

    pub(super) fn signals(self) -> Result<Signals, DbError> {
        let Self { pane, text_pool } = self;
        let text_pool = list_of::<StoredTextLine>(&text_pool, "text lines")?
            .into_iter()
            .map(|line| {
                Ok(TextLine {
                    origin: line.origin.parse().map_err(|_unknown| {
                        DbError::unreadable("a stored text line's origin", &line.origin)
                    })?,
                    text: line.text,
                })
            })
            .collect::<Result<_, DbError>>()?;
        Ok(Signals { pane: pane.signals()?, text_pool })
    }
}

#[cfg(test)]
#[path = "signal_rows_tests.rs"]
mod tests;
