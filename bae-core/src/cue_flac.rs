use std::path::Path;
use thiserror::Error;
use tracing::warn;
#[derive(Debug, Error)]
pub enum CueFlacError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("CUE parsing error: {0}")]
    CueParsing(String),
}

/// One `INDEX` point of a track, positioned in CUE frames (1/75th of a second).
///
/// Convert to samples with `frames * sample_rate / 75` — exact at every standard
/// rate. Convert to ms with `frames * 1000 / 75`, which is lossy: UI display only.
#[derive(Debug, Clone)]
pub struct CueIndex {
    pub number: u32,
    pub frames: u64,
    pub file_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CueTrackMode {
    Audio,
    Other(String),
}

impl CueTrackMode {
    pub fn is_audio(&self) -> bool {
        matches!(self, Self::Audio)
    }
}

#[derive(Debug, Clone)]
pub enum CuePregap {
    None,
    Audio(CueIndex),
    Silence { frames: u64 },
}

#[derive(Debug, Clone)]
pub struct CueTrack {
    pub number: u32,
    pub mode: CueTrackMode,
    pub title: Option<String>,
    pub performer: Option<String>,
    pub indexes: Vec<CueIndex>,
    /// Filename from the `FILE` directive that scopes this track. For
    /// single-FILE CUE sheets every track carries the same string; for
    /// multi-FILE sheets (one FILE per TRACK, the shape used by lossy-format
    /// rips that can't be concatenated without re-encoding) each track
    /// carries its own.
    pub file_reference: String,
    /// INDEX 01 position in CUE frames
    pub start_cue_frames: u64,
    /// Pregap source for this track.
    pub pregap: CuePregap,
    /// Next track's boundary in CUE frames (None for last track)
    pub end_cue_frames: Option<u64>,
}

impl CueTrack {
    pub fn is_playable_audio(&self) -> bool {
        self.mode.is_audio()
    }

    pub fn index(&self, number: u32) -> Option<&CueIndex> {
        self.indexes.iter().find(|index| index.number == number)
    }

    /// Where audio bytes begin, in CUE frames: INDEX 00 when the pregap is real
    /// audio, else INDEX 01.
    pub fn audio_start_cue_frames(&self) -> u64 {
        match &self.pregap {
            CuePregap::Audio(index) => index.frames,
            CuePregap::None | CuePregap::Silence { .. } => self.start_cue_frames,
        }
    }

    /// INDEX 00 position in CUE frames when the pregap is real audio in the
    /// container; `None` for no pregap or a generated (`PREGAP` directive) one.
    pub fn pregap_start_cue_frames(&self) -> Option<u64> {
        match &self.pregap {
            CuePregap::Audio(index) => Some(index.frames),
            CuePregap::None | CuePregap::Silence { .. } => None,
        }
    }

    /// Silent pregap length in CUE frames from a `PREGAP` directive.
    pub fn generated_pregap_frames(&self) -> Option<u64> {
        match self.pregap {
            CuePregap::Silence { frames } => Some(frames),
            CuePregap::None | CuePregap::Audio(_) => None,
        }
    }

    /// INDEX 01 position in ms. Lossy — UI display only, as with the three below.
    pub fn start_time_ms(&self) -> u64 {
        self.start_cue_frames * 1000 / 75
    }

    /// Where the track's audio bytes begin, in ms — the pregap when there is one.
    pub fn audio_start_ms(&self) -> u64 {
        self.audio_start_cue_frames() * 1000 / 75
    }

    pub fn end_time_ms(&self) -> Option<u64> {
        self.end_cue_frames.map(|f| f * 1000 / 75)
    }

    /// INDEX 01 to end, in ms — excludes the pregap.
    pub fn track_duration_ms(&self) -> Option<u64> {
        self.end_cue_frames
            .map(|end| (end * 1000 / 75).saturating_sub(self.start_time_ms()))
    }

    /// Pregap duration in ms; `None` when the track has no pregap (no INDEX 00,
    /// or the bogus-pregap corrector cleared it).
    pub fn pregap_duration_ms(&self) -> Option<u64> {
        self.pregap_start_cue_frames()
            .map(|pregap| self.start_time_ms().saturating_sub(pregap * 1000 / 75))
    }

    pub fn generated_pregap_duration_ms(&self) -> Option<u64> {
        self.generated_pregap_frames()
            .map(|frames| frames * 1000 / 75)
    }
}

#[derive(Debug, Clone)]
pub struct CueSheet {
    pub title: Option<String>,
    pub performer: Option<String>,
    /// Media catalog number from `CATALOG <13-digit>`. Typically the UPC/EAN
    /// barcode — a strong release identifier when matching against MusicBrainz.
    pub catalog: Option<String>,
    /// Year or date string from `REM DATE <value>` (rippers write whatever they
    /// have: a year like "2001", a range like "2000 / 2004", or a full date).
    pub date: Option<String>,
    pub tracks: Vec<CueTrack>,
}

impl CueSheet {
    pub fn playable_tracks(&self) -> impl Iterator<Item = &CueTrack> {
        self.tracks.iter().filter(|track| track.is_playable_audio())
    }

    pub fn playable_track_count(&self) -> usize {
        self.playable_tracks().count()
    }

    pub fn audio_file_references(&self) -> Vec<&str> {
        let mut refs = Vec::new();
        for track in self.playable_tracks() {
            for index in &track.indexes {
                if !refs.contains(&index.file_reference.as_str()) {
                    refs.push(index.file_reference.as_str());
                }
            }
            if !refs.contains(&track.file_reference.as_str()) {
                refs.push(track.file_reference.as_str());
            }
        }
        refs
    }

    /// `Some(filename)` if every track refers to the same `FILE` (single-FILE
    /// CUE — one concatenated audio container per release, the EAC shape).
    /// `None` for multi-FILE CUEs (one FILE per TRACK, the lossy-rip shape).
    /// The discriminator for "is this a CUE+single-audio pair candidate?".
    pub fn single_file(&self) -> Option<&str> {
        let first = self.playable_tracks().next()?.file_reference.as_str();
        self.playable_tracks()
            .all(|t| t.file_reference == first)
            .then_some(first)
    }
}

#[derive(Debug)]
struct PendingCueTrack {
    number: u32,
    mode: CueTrackMode,
    title: Option<String>,
    performer: Option<String>,
    indexes: Vec<CueIndex>,
    pregap: Option<(u64, String)>,
    generated_pregap_frames: Option<u64>,
    start: Option<(u64, String)>,
    file_reference: String,
}

impl PendingCueTrack {
    fn new(number: u32, mode: CueTrackMode, file_reference: String) -> Self {
        Self {
            number,
            mode,
            title: None,
            performer: None,
            indexes: Vec::new(),
            pregap: None,
            generated_pregap_frames: None,
            start: None,
            file_reference,
        }
    }

    fn finish(self) -> Result<CueTrack, CueFlacError> {
        let (start_cue_frames, file_reference) = match self.start {
            Some(start) => start,
            None if !self.mode.is_audio() => (0, self.file_reference.clone()),
            None => {
                return Err(CueFlacError::CueParsing(format!(
                    "TRACK {} has no INDEX 01",
                    self.number
                )));
            }
        };
        let pregap = match (self.pregap, self.generated_pregap_frames) {
            (Some(_), Some(_)) => {
                return Err(CueFlacError::CueParsing(format!(
                    "TRACK {} has both INDEX 00 and PREGAP",
                    self.number
                )));
            }
            (Some((frames, pregap_file_reference)), None) => CuePregap::Audio(CueIndex {
                number: 0,
                frames,
                file_reference: pregap_file_reference,
            }),
            (None, Some(frames)) => CuePregap::Silence { frames },
            (None, None) => CuePregap::None,
        };
        Ok(CueTrack {
            number: self.number,
            mode: self.mode,
            title: self.title,
            performer: self.performer,
            indexes: self.indexes,
            file_reference,
            start_cue_frames,
            pregap,
            end_cue_frames: None,
        })
    }
}
pub fn parse_cue_sheet(cue_path: &Path) -> Result<CueSheet, CueFlacError> {
    use tracing::{debug, error};
    debug!("Attempting to parse CUE sheet: {:?}", cue_path);
    debug!("CUE path exists: {}", cue_path.exists());
    debug!("CUE path absolute: {:?}", cue_path.canonicalize());
    let content = crate::text_encoding::read_text_file(cue_path)
        .map(|d| d.text)
        .map_err(|e| {
            error!(
                "Failed to read CUE file {:?}: {} (os error {})",
                cue_path,
                e,
                e.raw_os_error().unwrap_or(-1)
            );
            e
        })?;
    parse_cue_content(&content)
}
/// Parse CUE sheet content.
///
/// The header (TITLE, PERFORMER, CATALOG, REM, the initial FILE) is accepted in
/// any order with any subset present: downstream only needs track counts,
/// offsets, and file references, so demanding a fixed header shape would reject
/// real CUE files for nothing.
///
/// After the header come the TRACK entries. The FILE in scope applies to every
/// TRACK until the next FILE, and a FILE may appear *inside* a track body, not
/// only between tracks — a per-track rip puts a track's INDEX 00 pregap at the
/// tail of the previous file and its INDEX 01 at the head of the next, so the
/// FILE directive lands between the two INDEX lines. A track's `file_reference`
/// is therefore whichever FILE was current when its INDEX 01 was parsed: the file
/// we would actually play.
///
/// A TRACK before any FILE, or a TRACK body without INDEX 01, is a parse error.
fn parse_cue_content(input: &str) -> Result<CueSheet, CueFlacError> {
    let mut title: Option<String> = None;
    let mut performer: Option<String> = None;
    let mut catalog: Option<String> = None;
    let mut date: Option<String> = None;
    let mut current_file: Option<String> = None;
    let mut tracks: Vec<CueTrack> = Vec::new();
    let mut current_track: Option<PendingCueTrack> = None;

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let track_directive = parse_track_directive(line)?;
        if let Some((number, mode)) = track_directive {
            if let Some(track) = current_track.take() {
                tracks.push(track.finish()?);
            }
            let Some(file_reference) = current_file.clone() else {
                return Err(CueFlacError::CueParsing(format!(
                    "TRACK before any FILE: {line:?}"
                )));
            };
            current_track = Some(PendingCueTrack::new(number, mode, file_reference));
            continue;
        }

        let file_directive = parse_file_directive(line)?;
        if let Some(file) = file_directive {
            current_file = Some(file);
            continue;
        }

        match current_track.as_mut() {
            Some(track) => {
                let file_reference = current_file
                    .as_deref()
                    .expect("CUE TRACK parsing starts only after FILE is known");
                apply_track_line(line, track, file_reference)?;
            }
            None => {
                apply_header_line(line, &mut title, &mut performer, &mut catalog, &mut date);
            }
        }
    }

    if let Some(track) = current_track {
        tracks.push(track.finish()?);
    }
    if tracks.is_empty() {
        return Err(CueFlacError::CueParsing(
            "CUE sheet contains no tracks".into(),
        ));
    }

    // Some rippers write a bogus INDEX 00 that sits before the previous track's
    // INDEX 01. Drop those here, so every downstream boundary reader sees one
    // already-corrected shape.
    for i in 1..tracks.len() {
        let bogus = match &tracks[i].pregap {
            CuePregap::Audio(index) => index.frames <= tracks[i - 1].start_cue_frames,
            CuePregap::None | CuePregap::Silence { .. } => false,
        };
        if bogus {
            tracks[i].pregap = CuePregap::None;
            tracks[i].indexes.retain(|index| index.number != 0);
        }
    }
    for i in 0..tracks.len() {
        if i + 1 < tracks.len() {
            let next_track = &tracks[i + 1];
            let boundary_index = match &next_track.pregap {
                CuePregap::Audio(index) => Some(index),
                CuePregap::None | CuePregap::Silence { .. } => next_track.index(1),
            };
            tracks[i].end_cue_frames = boundary_index
                .filter(|index| index.file_reference == tracks[i].file_reference)
                .map(|index| index.frames);
        }
    }
    Ok(CueSheet {
        title,
        performer,
        catalog,
        date,
        tracks,
    })
}

fn apply_header_line(
    line: &str,
    title: &mut Option<String>,
    performer: &mut Option<String>,
    catalog: &mut Option<String>,
    date: &mut Option<String>,
) {
    match keyword_and_rest(line) {
        Some(("REM", rest)) => {
            if let Some((keyword, value)) = keyword_and_rest(rest) {
                if keyword == "DATE" {
                    *date = Some(strip_optional_quotes(value).to_string());
                }
            }
        }
        Some(("CATALOG", rest)) => {
            *catalog = Some(strip_optional_quotes(rest).to_string());
        }
        Some(("TITLE", rest)) => {
            if let Some(value) = quoted_value(rest) {
                *title = Some(value.to_string());
            }
        }
        Some(("PERFORMER", rest)) => {
            if let Some(value) = quoted_value(rest) {
                *performer = Some(value.to_string());
            }
        }
        Some((keyword, _)) if header_keyword_is_known_skipped(keyword) => {}
        _ => warn!("unrecognized CUE header line: {:?}", line),
    }
}

fn apply_track_line(
    line: &str,
    track: &mut PendingCueTrack,
    current_file: &str,
) -> Result<(), CueFlacError> {
    match keyword_and_rest(line) {
        Some(("REM", _)) => {}
        Some(("TITLE", rest)) => {
            if let Some(value) = quoted_value(rest) {
                track.title = Some(value.to_string());
            }
        }
        Some(("PERFORMER", rest)) => {
            if let Some(value) = quoted_value(rest) {
                track.performer = Some(value.to_string());
            }
        }
        Some(("INDEX", rest)) => {
            let (index_number, cue_frames) = parse_index_values(line, rest)?;
            track.indexes.push(CueIndex {
                number: index_number,
                frames: cue_frames,
                file_reference: current_file.to_string(),
            });
            match index_number {
                0 => track.pregap = Some((cue_frames, current_file.to_string())),
                1 => {
                    track.start = Some((cue_frames, current_file.to_string()));
                }
                _ => {}
            }
        }
        Some(("PREGAP", rest)) => {
            track.generated_pregap_frames = Some(parse_time(rest)?);
        }
        Some((keyword, _)) if track_keyword_is_known_skipped(keyword) => {}
        _ => warn!(
            "unrecognized CUE line in track {}: {:?}",
            track.number, line
        ),
    }
    Ok(())
}

fn parse_track_directive(line: &str) -> Result<Option<(u32, CueTrackMode)>, CueFlacError> {
    let Some(("TRACK", rest)) = keyword_and_rest(line) else {
        return Ok(None);
    };
    let mut parts = rest.split_whitespace();
    let number = parse_u32_token(line, parts.next())?;
    let mode = parts
        .next()
        .ok_or_else(|| CueFlacError::CueParsing(format!("TRACK line has no mode: {line:?}")))?;
    let mode = if mode == "AUDIO" {
        CueTrackMode::Audio
    } else {
        CueTrackMode::Other(mode.to_string())
    };
    Ok(Some((number, mode)))
}

fn parse_file_directive(line: &str) -> Result<Option<String>, CueFlacError> {
    let Some(("FILE", rest)) = keyword_and_rest(line) else {
        return Ok(None);
    };
    Ok(Some(parse_file_name_value(line, rest)?))
}

fn parse_index_values(line: &str, rest: &str) -> Result<(u32, u64), CueFlacError> {
    let mut parts = rest.split_whitespace();
    let index_number = parse_u32_token(line, parts.next())?;
    let time = parts
        .next()
        .ok_or_else(|| CueFlacError::CueParsing(format!("malformed INDEX line: {line:?}")))?;
    let cue_frames = parse_time(time)?;
    Ok((index_number, cue_frames))
}

fn parse_u32_token(line: &str, token: Option<&str>) -> Result<u32, CueFlacError> {
    token
        .ok_or_else(|| CueFlacError::CueParsing(format!("missing number in line: {line:?}")))?
        .parse::<u32>()
        .map_err(|_| CueFlacError::CueParsing(format!("invalid number in line: {line:?}")))
}

/// Spec commands valid inside a TRACK body that we don't store — listed so they
/// don't trip the unknown-line warning.
fn track_keyword_is_known_skipped(keyword: &str) -> bool {
    const TRACK_BODY_SKIPPED: &[&str] = &[
        "FLAGS",
        "COMPOSER",
        "SONGWRITER",
        "ISRC",
        "POSTGAP",
        "CDTEXTFILE",
        "ARRANGER",
        "GENRE",
        "MESSAGE",
        "DISC_ID",
        "TOC_INFO2",
        "TOC_INFO",
        "UPC_EAN",
        "SIZE_INFO",
    ];
    TRACK_BODY_SKIPPED.contains(&keyword)
}
/// The same, for the header. CD-Text keywords may appear at the global level too,
/// and CDTEXTFILE points at an external binary CD-Text file.
fn header_keyword_is_known_skipped(keyword: &str) -> bool {
    const HEADER_SKIPPED: &[&str] = &[
        "CDTEXTFILE",
        "SONGWRITER",
        "ARRANGER",
        "COMPOSER",
        "GENRE",
        "MESSAGE",
        "DISC_ID",
        "TOC_INFO2",
        "TOC_INFO",
        "UPC_EAN",
        "SIZE_INFO",
    ];
    HEADER_SKIPPED.contains(&keyword)
}

fn keyword_and_rest(input: &str) -> Option<(&str, &str)> {
    let input = input.trim_start();
    let end = input.find(char::is_whitespace).unwrap_or(input.len());
    if end == 0 {
        return None;
    }
    Some((&input[..end], input[end..].trim_start()))
}

fn quoted_value(input: &str) -> Option<&str> {
    let input = input.trim_start();
    let after_open = input.strip_prefix('"')?;
    let end = after_open.find('"')?;
    Some(&after_open[..end])
}

fn strip_optional_quotes(input: &str) -> &str {
    let input = input.trim();
    if let Some(value) = quoted_value(input) {
        value
    } else {
        input
    }
}

fn parse_file_name_value(line: &str, rest: &str) -> Result<String, CueFlacError> {
    let rest = rest.trim_start();
    if let Some(value) = quoted_value(rest) {
        return Ok(value.trim().to_string());
    }
    let name = rest
        .split_whitespace()
        .next()
        .ok_or_else(|| CueFlacError::CueParsing(format!("FILE line has no name: {line:?}")))?;
    Ok(name.to_string())
}

/// MM:SS:FF to total CUE frames (1/75th second). The frames field takes its
/// leading ASCII digits and ignores any trailer.
fn parse_time(input: &str) -> Result<u64, CueFlacError> {
    let mut parts = input.splitn(3, ':');
    let minutes = parse_u64_time_field(input, parts.next())?;
    let seconds = parse_u64_time_field(input, parts.next())?;
    let frames_and_rest = parts
        .next()
        .ok_or_else(|| CueFlacError::CueParsing(format!("invalid CUE time {input:?}")))?;
    let frame_end = frames_and_rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(frames_and_rest.len());
    if frame_end == 0 {
        return Err(CueFlacError::CueParsing(format!(
            "invalid CUE time {input:?}"
        )));
    }
    let frames = frames_and_rest[..frame_end]
        .parse::<u64>()
        .map_err(|_| CueFlacError::CueParsing(format!("invalid CUE time {input:?}")))?;
    Ok((minutes * 60 + seconds) * 75 + frames)
}

fn parse_u64_time_field(input: &str, token: Option<&str>) -> Result<u64, CueFlacError> {
    token
        .ok_or_else(|| CueFlacError::CueParsing(format!("invalid CUE time {input:?}")))?
        .parse::<u64>()
        .map_err(|_| CueFlacError::CueParsing(format!("invalid CUE time {input:?}")))
}
#[cfg(test)]
mod tests;
