//! What a candidate's rip artifacts prove about its medium — the CD rip a
//! log, an AccurateRip report or a ripper's track sheet proves, and the rate
//! of its audio that rules a CD out — and the discs its logs and sheets hash
//! to, read as one, since the first decides whether the second is asked.

use crate::cue_sheet::CueSheet;
use crate::discid::{calculate_mb_discid_from_cue, discid_from_log_text, SheetAudioDuration};
use crate::ImportError;
use bae_catalogs::rip::rate_ruling_out_cd;
use bae_catalogs::{AudioOrigin, AudioSource, CdProof};
use bae_identify::signals::{ComputedDiscId, DiscIdSignal};
use bae_records::DocumentKind;
use bae_types::audio_format::AudioFormat;
use std::path::Path;
use tracing::{debug, trace, warn};

/// A document's text, decoded whatever its encoding (an EAC log is UTF-16).
/// A file that cannot be read is an error, not a document that states
/// nothing: the evidence it holds is unknown.
fn read_document(path: &Path) -> Result<String, ImportError> {
    bae_util::text_encoding::read_text_file(path)
        .map(|read| read.text)
        .map_err(|error| ImportError::DocumentRead { file: path.display().to_string(), error })
}

/// Whether a document is an AccurateRip report that found the disc: a line
/// `[AccurateRip ID: <id>] found.`, as CUETools writes into its `.accurip`
/// report. AccurateRip's database holds CDs alone, so a disc found in it is
/// a CD.
fn reports_accuraterip_found(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .any(|line| line.starts_with("[AccurateRip ID:") && line.ends_with("] found."))
}

/// What a candidate's rip artifacts say: the CD rip they prove and the rate
/// that rules one out, and the disc they hash to. One reading, because the
/// first decides the second — a track sheet laying out audio no CD could hold
/// hashes to a disc that never existed, and is not asked about. A download is
/// read with the tags, and is never a source this sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RipReading {
    pub origin: AudioOrigin,
    pub disc_id: DiscIdReading,
}

impl RipReading {
    /// Audio of `lengths` whose rip logs and sheets read as this.
    pub(crate) fn read_audio(
        self,
        lengths: bae_catalogs::reading::MeasuredLengths,
    ) -> bae_catalogs::reading::ReadAudio {
        let disc_ids = self.disc_id.computed().into_iter().map(|disc| disc.disc_id).collect();
        bae_catalogs::reading::ReadAudio::new(lengths, self.origin, disc_ids)
    }
}

/// The disc IDs a candidate's rip artifacts hash to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscIdReading {
    /// One per disc the logs and sheets describe, in reading order. Never
    /// empty.
    Computed(Vec<ComputedDiscId>),
    /// No log or sheet hashes to one.
    Absent,
    /// A sheet was there and the audio rules a CD out (see
    /// [`AudioOrigin::not_cd_rate`]), so it was not hashed.
    NotCdAudio,
}

impl DiscIdReading {
    /// The disc IDs, where any was computed.
    pub fn computed(self) -> Vec<ComputedDiscId> {
        match self {
            Self::Computed(discs) => discs,
            Self::Absent | Self::NotCdAudio => Vec::new(),
        }
    }

    /// The signal this reading is.
    pub fn into_signal(self) -> DiscIdSignal {
        match self {
            Self::Computed(discs) => DiscIdSignal::Computed { discs },
            Self::Absent => DiscIdSignal::Absent,
            Self::NotCdAudio => DiscIdSignal::NotCdAudio,
        }
    }
}

/// A rip log or an AccurateRip report.
struct RipDocument<'a> {
    path: &'a Path,
    /// The candidate-relative path, where the document is a folder's file.
    file: Option<&'a str>,
    /// A rip log or an AccurateRip report.
    kind: DocumentKind,
}

/// A track sheet with the measured length of every audio file it names.
struct RipSheet<'a> {
    sheet: &'a CueSheet,
    audio: Vec<SheetAudioDuration<'a>>,
    path: &'a Path,
    file: Option<&'a str>,
}

/// Everything of a candidate's that speaks to where its audio came from.
struct RipArtifacts<'a> {
    /// The logs, then the AccurateRip reports.
    documents: Vec<RipDocument<'a>>,
    /// The sheets that lay out the candidate's tracks.
    sheets: Vec<RipSheet<'a>>,
    /// The format of every one of the candidate's audio files.
    audio: Vec<&'a AudioFormat>,
}

/// Read the artifacts. Every log whose table of contents reads is a disc ID
/// and the proof of a CD rip — the most accurate disc ID, since EAC and XLD
/// put the disc's sector offsets in it directly. Without one the proof is an
/// AccurateRip report or a sheet a CD ripper wrote. The audio's rate is read
/// whatever proves a rip, and the sheets are hashed unless the rate rules a
/// CD out and nothing proves one — every sheet but one beside a log that
/// read, which describes the disc that log already hashed. A folder holding
/// one rip per disc yields one disc ID per disc; one hashed twice is kept
/// once. A document that cannot be read fails the reading, as does a log or
/// sheet that cannot be hashed to a disc; a log that reads but holds no table
/// of contents states no disc, and is passed over, logged.
fn read(artifacts: RipArtifacts<'_>) -> Result<RipReading, ImportError> {
    let not_cd_rate = rate_ruling_out_cd(artifacts.audio.iter().copied());
    let mut discs: Vec<ComputedDiscId> = Vec::new();
    let mut read_logs: Vec<&RipDocument<'_>> = Vec::new();
    let mut report = None;
    for document in &artifacts.documents {
        let text = read_document(document.path)?;
        if document.kind == DocumentKind::RipLog {
            trace!("Reading LOG file: {:?}", document.path);
            let disc_id = discid_from_log_text(&text).map_err(|error| {
                ImportError::DiscIdCalculation { file: document.path.display().to_string(), error }
            })?;
            match disc_id {
                Some(disc_id) => {
                    keep_disc(&mut discs, disc_id, document.file);
                    read_logs.push(document);
                    continue;
                }
                None => warn!(
                    "LOG {:?} holds no table of contents, so it states no disc",
                    document.path
                ),
            }
        }
        if report.is_none() && reports_accuraterip_found(&text) {
            report = Some(document.file);
        }
    }

    let source = match (read_logs.first(), report) {
        (Some(log), _) => {
            Some(AudioSource::CdRip { proof: CdProof::RipLog, file: log.file.map(str::to_string) })
        }
        (None, Some(file)) => Some(AudioSource::CdRip {
            proof: CdProof::AccurateRipReport,
            file: file.map(str::to_string),
        }),
        (None, None) => {
            artifacts.sheets.iter().find(|sheet| sheet.sheet.ripper.is_some()).map(|sheet| {
                AudioSource::CdRip {
                    proof: CdProof::RipperSheet,
                    file: sheet.file.map(str::to_string),
                }
            })
        }
    };
    let origin = AudioOrigin { source, not_cd_rate };

    if origin.not_cd_rate.is_some() && !origin.is_cd_rip() {
        // Nothing proves a CD rip, so no log read.
        let disc_id = if artifacts.sheets.is_empty() {
            DiscIdReading::Absent
        } else {
            debug!("not hashing a track sheet whose audio no CD holds");
            DiscIdReading::NotCdAudio
        };
        return Ok(RipReading { origin, disc_id });
    }
    for sheet in &artifacts.sheets {
        if read_logs.iter().any(|log| log.path.parent() == sheet.path.parent()) {
            debug!("not hashing {:?}: a log beside it already hashed its disc", sheet.path);
            continue;
        }
        let disc_id = discid_from_cue_audio(sheet.sheet, &sheet.audio, sheet.path)?;
        keep_disc(&mut discs, disc_id, sheet.file);
    }
    let disc_id =
        if discs.is_empty() { DiscIdReading::Absent } else { DiscIdReading::Computed(discs) };
    Ok(RipReading { origin, disc_id })
}

/// Add a disc ID read off `file`, unless another file already hashed to it.
fn keep_disc(discs: &mut Vec<ComputedDiscId>, disc_id: String, file: Option<&str>) {
    if let Some(kept) = discs.iter().find(|kept| kept.disc_id == disc_id) {
        debug!(
            disc_id,
            kept_from = ?kept.source_file,
            again_from = ?file,
            "two rip files hash to one Disc ID; it is kept once"
        );
        return;
    }
    discs.push(ComputedDiscId { disc_id, source_file: file.map(str::to_string) });
}

/// A MusicBrainz DiscID from an already-parsed CUE sheet and the lengths the
/// authoritative scan retained for the audio it names. A sheet that cannot be
/// laid out as a disc fails the reading: its Disc ID is unknown, which is not
/// a folder without one.
fn discid_from_cue_audio(
    sheet: &CueSheet,
    audio: &[SheetAudioDuration<'_>],
    cue_path: &Path,
) -> Result<String, ImportError> {
    trace!("Retained audio lengths for {:?}: {:?}", cue_path, audio);
    calculate_mb_discid_from_cue(sheet, audio, "CUE/scanned audio").map_err(|error| {
        ImportError::DiscIdCalculation { file: cue_path.display().to_string(), error }
    })
}

/// Read already-categorized files, reusing the track sheets the folder scan
/// parsed — no re-read, no re-parse. Only the sheets that carve are read: one
/// the person took out of the tracklist describes a disc this folder is no
/// longer presenting. A folder whose sheet is unbound can still identify
/// itself from its log. Each disc and proof names the file it was read off.
pub fn read_rip_artifacts(
    categorized: &crate::folder_scanner::CategorizedFiles,
) -> Result<RipReading, ImportError> {
    read_categorized(categorized, |file| Some(&file.relative_path))
}

/// Read a library release's files as [`read_rip_artifacts`] reads a folder's
/// (see [`crate::folder_scanner::categorize_library_files`]). A library
/// release's signals name none of its files.
pub(crate) fn read_library_rip_artifacts(
    categorized: &crate::folder_scanner::CategorizedFiles,
) -> Result<RipReading, ImportError> {
    read_categorized(categorized, |_| None)
}

/// Read `categorized`'s rip files, `named` giving the name each disc and
/// proof carries of the file it was read off.
fn read_categorized(
    categorized: &crate::folder_scanner::CategorizedFiles,
    named: fn(&crate::folder_scanner::ScannedFile) -> Option<&str>,
) -> Result<RipReading, ImportError> {
    // Logs first: a log's table of contents is the Disc ID as well as the
    // proof, which a report is not.
    let rip_documents = |kind| {
        categorized.documents_of(kind).map(move |doc| RipDocument {
            path: &doc.version.path,
            file: named(doc),
            kind,
        })
    };
    let documents = rip_documents(DocumentKind::RipLog)
        .chain(rip_documents(DocumentKind::AccurateRipReport))
        .collect();
    let carving = categorized.carving_sheets();
    let sheets = carving
        .iter()
        .map(|bound| RipSheet {
            sheet: bound.sheet,
            audio: bound
                .audio_files
                .iter()
                .map(|(file_reference, audio)| SheetAudioDuration {
                    file_reference,
                    duration_ms: audio
                        .source_audio
                        .as_ref()
                        .expect("a categorized audio file retains its scan facts")
                        .duration_ms,
                })
                .collect(),
            path: &bound.file.version.path,
            file: named(bound.file),
        })
        .collect();
    read(RipArtifacts {
        documents,
        sheets,
        audio: categorized
            .audio()
            .filter_map(|file| file.source_audio.as_ref())
            .map(|audio| &audio.format)
            .collect(),
    })
}

#[cfg(test)]
#[path = "rip_reading_tests.rs"]
mod tests;
