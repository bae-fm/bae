use crate::album_detail::AudioFormat;
use crate::cue_flac::CueSheet;
use crate::import::folder_scanner::resolve_cue_audio_paths;
use crate::signals::rip::rate_ruling_out_cd;
use crate::signals::{CdProof, DiscIdSignal, RipEvidence};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, trace, warn};

const CD_PREGAP_SECTORS: i32 = 150;

fn invalid_discid_data(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}

fn parse_log_toc_row(line: &str) -> Option<(i32, i32)> {
    let parts: Vec<&str> = line.split('|').collect();
    if parts.len() < 5 {
        return None;
    }

    let track_num = parts[0].trim().parse::<u32>().ok()?;
    if !(1..=99).contains(&track_num) {
        return None;
    }

    let start_sector = parts[3].trim().parse::<i32>().ok()?;
    let end_sector = parts[4].trim().parse::<i32>().ok()?;
    if start_sector < 0 || end_sector <= 0 {
        return None;
    }

    Some((start_sector, end_sector))
}

/// Extract raw `(start_sector, end_sector)` pairs from an EAC/XLD LOG TOC table.
/// Format: "       10  | 37:42.72 |  4:14.43 |    169722    |   188814"
fn extract_log_toc_sectors(log_content: &str) -> Result<Vec<(i32, i32)>, std::io::Error> {
    trace!("Parsing LOG file TOC");
    let mut in_toc_section = false;
    let mut track_sectors = Vec::new();
    for line in log_content.lines() {
        let line = line.trim();
        let line_lower = line.to_ascii_lowercase();
        let toc_row = parse_log_toc_row(line);

        if line_lower.contains("toc")
            && (line_lower.contains("cd") || line_lower.contains("extracted"))
        {
            in_toc_section = true;
            trace!("Found TOC section header: {}", line);
            continue;
        }
        if !in_toc_section && toc_row.is_some() {
            in_toc_section = true;
            trace!("Found TOC table format directly (no header)");
        }
        if in_toc_section
            && (line_lower.contains("range status")
                || line_lower.contains("accuraterip")
                || (line.is_empty() && !track_sectors.is_empty()))
        {
            trace!("End of TOC section, found {} tracks", track_sectors.len());
            break;
        }
        if !in_toc_section {
            continue;
        }
        if line.contains("---")
            || line.is_empty()
            || (line_lower.contains("track")
                && (line_lower.contains("start") || line_lower.contains("sector")))
        {
            continue;
        }
        if let Some((start_sector, end_sector)) = toc_row {
            track_sectors.push((start_sector, end_sector));
            trace!(
                "  Track {} sectors: start={}, end={}",
                track_sectors.len(),
                start_sector,
                end_sector
            );
        }
    }
    if track_sectors.is_empty() {
        warn!("Could not find any TOC rows in LOG file");
        let toc_start = log_content.lines().position(|l| {
            let l_lower = l.to_ascii_lowercase();
            l_lower.contains("toc") && (l_lower.contains("cd") || l_lower.contains("extracted"))
        });
        let preview: String = if let Some(start_idx) = toc_start {
            log_content
                .lines()
                .skip(start_idx)
                .take(15)
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            log_content.lines().take(30).collect::<Vec<_>>().join("\n")
        };
        debug!("LOG content preview (TOC section):\n{}", preview);
        return Err(invalid_discid_data("No TOC rows found in LOG file"));
    }

    Ok(track_sectors)
}

fn discid_from_raw_offsets(
    method_label: &str,
    raw_track_sectors: &[i32],
    raw_leadout_sector: i32,
    leadout_source: &str,
) -> Result<String, std::io::Error> {
    let track_offsets: Vec<i32> = raw_track_sectors
        .iter()
        .map(|sector| {
            sector.checked_add(CD_PREGAP_SECTORS).ok_or_else(|| {
                invalid_discid_data("track start sector out of range for DiscID calculation")
            })
        })
        .collect::<Result<_, _>>()?;
    let lead_out_sectors = raw_leadout_sector
        .checked_add(CD_PREGAP_SECTORS)
        .ok_or_else(|| {
            invalid_discid_data("lead-out sector out of range for DiscID calculation")
        })?;
    debug!(
        "{method_label} raw track start sectors before adding 150: {:?}",
        raw_track_sectors
    );
    debug!("{method_label} raw lead-out sector {leadout_source}: {raw_leadout_sector}");
    debug!(
        "{method_label} lead-out offset: {lead_out_sectors} sectors (raw: {raw_leadout_sector} + 150)"
    );

    let mut offsets = Vec::with_capacity(track_offsets.len() + 1);
    offsets.push(lead_out_sectors);
    offsets.extend_from_slice(&track_offsets);

    debug!(
        "{method_label} DiscID offsets: first_track=1, last_track={}, offsets={:?}",
        track_offsets.len(),
        offsets
    );

    let mb_discid_str = super::discid_hash::musicbrainz_discid(&offsets)
        .map_err(|e| invalid_discid_data(format!("Failed to calculate DiscID: {e}")))?;
    debug!("MusicBrainz DiscID calculated: {}", mb_discid_str);
    Ok(mb_discid_str)
}

/// MusicBrainz DiscID from a rip log's text alone — the most direct method,
/// since the sector offsets are in the log and neither the CUE nor the audio
/// is needed.
fn discid_from_log_text(log_content: &str) -> Result<String, std::io::Error> {
    trace!("LOG file decoded, length: {} chars", log_content.len());
    let toc_sectors = extract_log_toc_sectors(log_content)?;
    let raw_track_sectors: Vec<i32> = toc_sectors.iter().map(|(start, _)| *start).collect();
    let last_end_sector = toc_sectors
        .last()
        .expect("LOG TOC parser returned at least one row")
        .1;
    let raw_leadout_sector = last_end_sector
        .checked_add(1)
        .ok_or_else(|| invalid_discid_data("lead-out sector out of range in LOG TOC"))?;
    debug!("Found {} track(s) in LOG file", raw_track_sectors.len());
    discid_from_raw_offsets(
        "LOG",
        &raw_track_sectors,
        raw_leadout_sector,
        "from LOG lead-out",
    )
}
/// The measured length of one audio file a sheet names.
#[derive(Debug, Clone, Copy)]
pub struct SheetAudioDuration<'a> {
    /// The path as the sheet's `FILE` directive spells it.
    pub file_reference: &'a str,
    pub duration_ms: u64,
}

/// A file's length in CD sectors. A rip is a whole number of sectors, so
/// each file rounds on its own: rounding a sum of millisecond lengths would
/// let truncation drift across many files.
fn sectors_of(duration_ms: u64) -> u64 {
    (duration_ms * 75 + 500) / 1000
}

/// The disc a sheet and its audio describe, as MusicBrainz hashes it: every
/// audio track's INDEX 01 as a sector on the disc, and the lead-out after the
/// last, each offset including the 150-sector lead-in.
///
/// The sheet lays the disc out across its files in `FILE` order — one file
/// for the whole disc, or one per track. A track starts where its INDEX 01
/// sits inside its file, after every file before that one, and after the
/// silence every `PREGAP` directive up to and including its own generates:
/// that silence is on the disc and in no file. The lead-out is everything
/// laid end to end.
fn calculate_mb_discid_from_cue(
    sheet: &CueSheet,
    audio: &[SheetAudioDuration<'_>],
    method_label: &str,
) -> Result<String, std::io::Error> {
    if sheet.playable_tracks().next().is_none() {
        return Err(invalid_discid_data("CUE has no playable audio tracks"));
    }
    // Where each file begins on the disc.
    let mut file_start: HashMap<&str, u64> = HashMap::new();
    let mut laid = 0u64;
    for file_reference in sheet.audio_file_references() {
        let duration = audio
            .iter()
            .find(|duration| duration.file_reference == file_reference)
            .ok_or_else(|| {
                invalid_discid_data(format!(
                    "CUE FILE {file_reference:?} has no measured length"
                ))
            })?;
        file_start.insert(file_reference, laid);
        laid += sectors_of(duration.duration_ms);
    }
    let sector = |frames: u64| {
        i32::try_from(frames)
            .map_err(|_| invalid_discid_data("CUE lays the disc out past the sector range"))
    };
    let mut generated = 0u64;
    let mut raw_track_sectors = Vec::with_capacity(sheet.playable_track_count());
    for track in sheet.playable_tracks() {
        generated += track.generated_pregap_frames().unwrap_or(0);
        raw_track_sectors.push(sector(
            file_start[track.file_reference.as_str()] + track.start_cue_frames + generated,
        )?);
    }
    let raw_leadout_sector = sector(laid + generated)?;
    debug!(
        "Found {} track(s) across {} file(s) in CUE file",
        raw_track_sectors.len(),
        file_start.len()
    );
    discid_from_raw_offsets(
        method_label,
        &raw_track_sectors,
        raw_leadout_sector,
        "from the audio laid end to end",
    )
}

/// A document's text, decoded whatever its encoding (an EAC log is UTF-16).
/// `None` for a file that cannot be read, which is logged.
fn read_document(path: &Path) -> Option<String> {
    match crate::text_encoding::read_text_file(path) {
        Ok(read) => Some(read.text),
        Err(e) => {
            debug!("rip document {:?} could not be read: {}", path, e);
            None
        }
    }
}

/// Whether a document is an AccurateRip report that found the disc: a line
/// `[AccurateRip ID: <id>] found.`, as CUETools writes into its `.accurip`
/// report. AccurateRip's database holds CDs alone, so a disc found in it is
/// a CD.
fn reports_accuraterip_found(text: &str) -> bool {
    text.lines().map(str::trim).any(|line| {
        line.starts_with("[AccurateRip ID:") && line.ends_with("] found.")
    })
}

/// A disc ID and the file it was derived from — the rip log, or the sheet that
/// carves the tracks. The file rides along so a surface can put the disc ID on
/// the row for that file rather than beside the release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputedDiscId {
    pub disc_id: String,
    /// The candidate-relative path of the LOG or CUE it came from. `None`
    /// where the artifact is not a file of a scanned folder — a library
    /// release's own files, resolved for a re-identify pass, which no row
    /// points at.
    pub source_file: Option<String>,
}

/// What a candidate's rip artifacts say: the medium they prove or rule out,
/// and the disc they hash to. One reading, because the first decides the
/// second — a track sheet laying out audio no CD could hold hashes to a disc
/// that never existed, and is not asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RipReading {
    pub evidence: RipEvidence,
    pub disc_id: DiscIdReading,
}

/// The disc ID a candidate's rip artifacts hash to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscIdReading {
    Computed(ComputedDiscId),
    /// No log or sheet hashes to one.
    Absent,
    /// A sheet was there and the audio rules a CD out (see
    /// [`RipEvidence::NotCd`]), so it was not hashed.
    NotCdAudio { sample_rate_hz: u32 },
}

impl DiscIdReading {
    /// The disc ID, where one was computed.
    pub fn computed(self) -> Option<ComputedDiscId> {
        match self {
            Self::Computed(computed) => Some(computed),
            Self::Absent | Self::NotCdAudio { .. } => None,
        }
    }

    /// The signal a candidate of `track_count` tracks carries.
    pub fn into_signal(self, track_count: u32) -> DiscIdSignal {
        match self {
            Self::Computed(computed) => DiscIdSignal::Computed {
                disc_id: computed.disc_id,
                track_count,
                source_file: computed.source_file,
            },
            Self::Absent => DiscIdSignal::Absent { track_count },
            Self::NotCdAudio { sample_rate_hz } => DiscIdSignal::NotCdAudio {
                track_count,
                sample_rate_hz,
            },
        }
    }
}

/// A document that may be a rip log or an AccurateRip report.
struct RipDocument<'a> {
    path: &'a Path,
    /// The candidate-relative path, where the document is a folder's file.
    file: Option<&'a str>,
}

impl RipDocument<'_> {
    fn is_log(&self) -> bool {
        has_extension(self.path, "log")
    }
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
    /// The logs and AccurateRip reports, in the order found.
    documents: Vec<RipDocument<'a>>,
    /// The sheets that lay out the candidate's tracks.
    sheets: Vec<RipSheet<'a>>,
    /// The format of every one of the candidate's audio files.
    audio: Vec<&'a AudioFormat>,
}

fn has_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(expected))
}

/// Whether a document is one a rip reading reads: a rip log, or an
/// AccurateRip report.
pub(crate) fn is_rip_document(path: &Path) -> bool {
    has_extension(path, "log") || has_extension(path, "accurip")
}

/// Read the artifacts. A log whose table of contents reads is both the proof
/// and the disc ID, and is read first — most accurate, since EAC and XLD put
/// the disc's sector offsets in it directly. Otherwise the proof is an
/// AccurateRip report or a sheet a CD ripper wrote, or the audio's rate rules
/// a CD out; and the sheets are hashed unless it does. Failures along the
/// way log at `debug!` so the chain shows up in traces.
fn read(mut artifacts: RipArtifacts<'_>) -> RipReading {
    // Logs first: a log's table of contents is the disc ID as well as the
    // proof, which a report is not.
    artifacts.documents.sort_by_key(|document| !document.is_log());
    let mut report = None;
    for document in &artifacts.documents {
        let Some(text) = read_document(document.path) else {
            continue;
        };
        if document.is_log() {
            trace!("Reading LOG file: {:?}", document.path);
            match discid_from_log_text(&text) {
                Ok(disc_id) => {
                    let file = document.file.map(str::to_string);
                    return RipReading {
                        evidence: RipEvidence::Cd {
                            proof: CdProof::RipLog,
                            file: file.clone(),
                        },
                        disc_id: DiscIdReading::Computed(ComputedDiscId {
                            disc_id,
                            source_file: file,
                        }),
                    };
                }
                Err(e) => debug!("DiscID from LOG failed for {:?}: {}", document.path, e),
            }
        }
        if report.is_none() && reports_accuraterip_found(&text) {
            report = Some(document.file);
        }
    }

    let evidence = match report {
        Some(file) => RipEvidence::Cd {
            proof: CdProof::AccurateRipReport,
            file: file.map(str::to_string),
        },
        None => match artifacts
            .sheets
            .iter()
            .find(|sheet| sheet.sheet.ripper.is_some())
        {
            Some(sheet) => RipEvidence::Cd {
                proof: CdProof::RipperSheet,
                file: sheet.file.map(str::to_string),
            },
            None => match rate_ruling_out_cd(artifacts.audio.iter().copied()) {
                Some(sample_rate_hz) => RipEvidence::NotCd { sample_rate_hz },
                None => RipEvidence::Unproven,
            },
        },
    };

    let disc_id = match evidence {
        RipEvidence::NotCd { sample_rate_hz } if !artifacts.sheets.is_empty() => {
            debug!("not hashing a track sheet whose audio is at {sample_rate_hz} Hz");
            DiscIdReading::NotCdAudio { sample_rate_hz }
        }
        RipEvidence::NotCd { .. } => DiscIdReading::Absent,
        RipEvidence::Cd { .. } | RipEvidence::Unproven => artifacts
            .sheets
            .iter()
            .find_map(|sheet| {
                discid_from_cue_audio(sheet.sheet, &sheet.audio, sheet.path).map(|disc_id| {
                    DiscIdReading::Computed(ComputedDiscId {
                        disc_id,
                        source_file: sheet.file.map(str::to_string),
                    })
                })
            })
            .unwrap_or(DiscIdReading::Absent),
    };
    RipReading { evidence, disc_id }
}

/// One of a library release's audio files: where it is, how long it plays,
/// and its format.
pub struct ReleaseAudioFile {
    pub path: PathBuf,
    pub duration_ms: u64,
    pub format: AudioFormat,
}

/// Read a library release's rip artifacts from their resolved paths:
/// `documents` its logs and AccurateRip reports, `cue_paths` its sheets, which
/// are parsed here and matched to `audio` by the names their `FILE`
/// directives give.
pub fn read_rip_artifacts_from_paths(
    documents: &[PathBuf],
    cue_paths: &[PathBuf],
    audio: &[ReleaseAudioFile],
) -> RipReading {
    let audio_paths = audio
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let parsed: Vec<(&PathBuf, CueSheet)> = cue_paths
        .iter()
        .filter_map(|cue_path| match crate::cue_flac::parse_cue_sheet(cue_path) {
            Ok(sheet) => Some((cue_path, sheet)),
            Err(e) => {
                debug!("Skipping unparseable CUE {:?}: {}", cue_path, e);
                None
            }
        })
        .collect();
    let sheets = parsed
        .iter()
        .filter_map(|(cue_path, sheet)| {
            let Some(resolved) = resolve_cue_audio_paths(cue_path, sheet, &audio_paths) else {
                debug!("Skipping CUE with no matching audio file: {:?}", cue_path);
                return None;
            };
            let audio = resolved
                .into_iter()
                .map(|(file_reference, audio_path)| SheetAudioDuration {
                    file_reference,
                    duration_ms: audio
                        .iter()
                        .find(|file| &file.path == audio_path)
                        .expect("a matched CUE audio path came from the release's audio")
                        .duration_ms,
                })
                .collect();
            Some(RipSheet {
                sheet,
                audio,
                path: cue_path.as_path(),
                file: None,
            })
        })
        .collect();
    read(RipArtifacts {
        documents: documents
            .iter()
            .filter(|path| is_rip_document(path))
            .map(|path| RipDocument { path, file: None })
            .collect(),
        sheets,
        audio: audio.iter().map(|file| &file.format).collect(),
    })
}

/// A MusicBrainz DiscID from an already-parsed CUE sheet and the lengths the
/// authoritative scan retained for the audio it names.
fn discid_from_cue_audio(
    sheet: &CueSheet,
    audio: &[SheetAudioDuration<'_>],
    cue_path: &Path,
) -> Option<String> {
    trace!("Retained audio lengths for {:?}: {:?}", cue_path, audio);
    match calculate_mb_discid_from_cue(sheet, audio, "CUE/scanned audio") {
        Ok(id) => Some(id),
        Err(e) => {
            debug!("DiscID from CUE+audio failed for {:?}: {}", cue_path, e);
            None
        }
    }
}

/// Read already-categorized files, reusing the track sheets the folder scan
/// parsed — no re-read, no re-parse. Only the sheets that carve are read: one
/// the person took out of the tracklist describes a disc this folder is no
/// longer presenting. A folder whose sheet is unbound can still identify
/// itself from its log.
pub fn read_rip_artifacts(
    categorized: &crate::import::folder_scanner::CategorizedFiles,
) -> RipReading {
    let documents = categorized
        .documents()
        .filter(|doc| is_rip_document(&doc.path))
        .map(|doc| RipDocument {
            path: &doc.path,
            file: Some(&doc.relative_path),
        })
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
            path: &bound.file.path,
            file: Some(&bound.file.relative_path),
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
#[path = "discid_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "rip_reading_tests.rs"]
mod rip_reading_tests;
