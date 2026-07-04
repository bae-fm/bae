use nom::IResult;
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

/// Represents a single track in a CUE sheet.
///
/// All positions are stored in CUE frames (1/75th of a second).
/// Convert to samples with `cue_frames * sample_rate / 75` (exact for all standard rates).
/// Convert to ms with `cue_frames * 1000 / 75` (lossy, for UI display only).
#[derive(Debug, Clone)]
pub struct CueTrack {
    pub number: u32,
    pub title: Option<String>,
    pub performer: Option<String>,
    /// 12-character International Standard Recording Code from `ISRC <code>`.
    pub isrc: Option<String>,
    /// Filename from the `FILE` directive that scopes this track. For
    /// single-FILE CUE sheets every track carries the same string; for
    /// multi-FILE sheets (one FILE per TRACK, the shape used by lossy-format
    /// rips that can't be concatenated without re-encoding) each track
    /// carries its own.
    pub file_reference: String,
    /// INDEX 01 position in CUE frames
    pub start_cue_frames: u64,
    /// INDEX 00 position in CUE frames (pregap start)
    pub pregap_cue_frames: Option<u64>,
    /// Next track's boundary in CUE frames (None for last track)
    pub end_cue_frames: Option<u64>,
}

impl CueTrack {
    /// Where audio bytes begin (CUE frames): INDEX 00 if pregap exists, else INDEX 01
    pub fn audio_start_cue_frames(&self) -> u64 {
        self.pregap_cue_frames.unwrap_or(self.start_cue_frames)
    }

    /// Where audio bytes begin, as sample position
    pub fn audio_start_sample(&self, sample_rate: u32) -> u64 {
        self.audio_start_cue_frames() * sample_rate as u64 / 75
    }

    /// End sample position (None for last track)
    pub fn end_sample(&self, sample_rate: u32) -> Option<u64> {
        self.end_cue_frames.map(|f| f * sample_rate as u64 / 75)
    }

    /// INDEX 01 position in milliseconds (lossy, for UI display only)
    pub fn start_time_ms(&self) -> u64 {
        self.start_cue_frames * 1000 / 75
    }

    /// Where audio bytes begin in ms (lossy, for UI display only)
    pub fn audio_start_ms(&self) -> u64 {
        self.audio_start_cue_frames() * 1000 / 75
    }

    /// End time in ms (lossy, for UI display only)
    pub fn end_time_ms(&self) -> Option<u64> {
        self.end_cue_frames.map(|f| f * 1000 / 75)
    }

    /// Duration from INDEX 01 to end in ms (for UI display, excludes pregap)
    pub fn track_duration_ms(&self) -> Option<u64> {
        self.end_cue_frames
            .map(|end| (end * 1000 / 75).saturating_sub(self.start_time_ms()))
    }

    /// Pregap duration in ms; `None` when the track has no pregap (no INDEX 00,
    /// or the bogus-pregap corrector cleared it).
    pub fn pregap_duration_ms(&self) -> Option<u64> {
        self.pregap_cue_frames
            .map(|pregap| self.start_time_ms().saturating_sub(pregap * 1000 / 75))
    }
}

/// Represents a parsed CUE sheet
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
    /// `Some(filename)` if every track refers to the same `FILE` (single-FILE
    /// CUE — one concatenated audio container per release, the EAC shape).
    /// `None` for multi-FILE CUEs (one FILE per TRACK, the lossy-rip shape).
    /// The discriminator for "is this a CUE+single-audio pair candidate?".
    pub fn single_file(&self) -> Option<&str> {
        let first = self.tracks.first()?.file_reference.as_str();
        self.tracks
            .iter()
            .all(|t| t.file_reference == first)
            .then_some(first)
    }
}
/// Represents a CUE/FLAC pair found during import
#[derive(Debug, Clone)]
pub struct CueFlacPair {
    pub audio_path: std::path::PathBuf,
    pub cue_path: std::path::PathBuf,
}

#[derive(Debug)]
struct PendingCueTrack {
    number: u32,
    title: Option<String>,
    performer: Option<String>,
    isrc: Option<String>,
    pregap_cue_frames: Option<u64>,
    start: Option<(u64, String)>,
}

impl PendingCueTrack {
    fn new(number: u32) -> Self {
        Self {
            number,
            title: None,
            performer: None,
            isrc: None,
            pregap_cue_frames: None,
            start: None,
        }
    }

    fn finish(self, input: &str) -> Result<CueTrack, nom::Err<nom::error::Error<&str>>> {
        let (start_cue_frames, file_reference) = self.start.ok_or_else(|| {
            nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Tag))
        })?;
        Ok(CueTrack {
            number: self.number,
            title: self.title,
            performer: self.performer,
            isrc: self.isrc,
            file_reference,
            start_cue_frames,
            pregap_cue_frames: self.pregap_cue_frames,
            end_cue_frames: None,
        })
    }
}
/// Main processor for CUE/FLAC operations
pub struct CueFlacProcessor;
impl CueFlacProcessor {
    /// Detect CUE/FLAC pairs from a list of file paths (no filesystem traversal)
    pub fn detect_cue_flac_from_paths(
        file_paths: &[std::path::PathBuf],
    ) -> Result<Vec<CueFlacPair>, CueFlacError> {
        let mut pairs = Vec::new();
        let mut audio_files = Vec::new();
        let mut cue_files = Vec::new();
        for path in file_paths {
            if let Some(extension) = path.extension() {
                let ext_lower = extension.to_str().map(|s| s.to_lowercase());
                match ext_lower.as_deref() {
                    Some("flac") | Some("ape") | Some("m4a") | Some("wav") | Some("aif")
                    | Some("aiff") | Some("aifc") | Some("wv") | Some("dsf") | Some("dff") => {
                        audio_files.push(path.clone())
                    }
                    Some("cue") => cue_files.push(path.clone()),
                    _ => {}
                }
            }
        }
        for cue_path in cue_files {
            let Some(cue_stem) = cue_path.file_stem().and_then(|s| s.to_str()) else {
                tracing::warn!("CUE path has no UTF-8 stem, skipping: {:?}", cue_path);
                continue;
            };
            let cue_dir = cue_path.parent();
            for audio_path in &audio_files {
                let Some(audio_stem) = audio_path.file_stem().and_then(|s| s.to_str()) else {
                    tracing::warn!("audio path has no UTF-8 stem, skipping: {:?}", audio_path);
                    continue;
                };
                if cue_stem == audio_stem && cue_dir == audio_path.parent() {
                    pairs.push(CueFlacPair {
                        audio_path: audio_path.clone(),
                        cue_path: cue_path.clone(),
                    });
                    break;
                }
            }
        }
        Ok(pairs)
    }
    /// Parse a CUE sheet file
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
        match Self::parse_cue_content(&content) {
            Ok((_, cue_sheet)) => Ok(cue_sheet),
            Err(e) => Err(CueFlacError::CueParsing(format!(
                "Failed to parse CUE: {}",
                e
            ))),
        }
    }
    /// Parse CUE sheet content.
    ///
    /// The top-level header (TITLE, PERFORMER, CATALOG, REM, and the initial
    /// FILE) is accepted in any order with any subset present — the scanner
    /// and importer only need track counts, offsets, and file references, so
    /// requiring a specific header shape would reject real CUE files
    /// unnecessarily. After the header, a sequence of `AUDIO` TRACK entries
    /// follows; the currently-scoped FILE applies to every TRACK that
    /// follows until the next FILE. A FILE may appear *inside* a track body,
    /// not only between tracks (per-track rips put each track's INDEX 00
    /// pregap at the tail of the previous file and INDEX 01 start at the
    /// head of the next — the FILE directive sits between the two INDEX
    /// lines). A track's `file_reference` is whichever FILE was current at
    /// the moment its INDEX 01 was parsed — the file we'd actually play.
    /// `single_file()` discriminates single-FILE (the EAC concatenated-audio
    /// shape) from multi-FILE (one FILE per TRACK, common in lossy-format
    /// rips). Malformed entries — TRACK before any FILE, or a TRACK body
    /// that lacks INDEX 01 — are a parse error.
    fn parse_cue_content(input: &str) -> IResult<&str, CueSheet> {
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

            let track_directive = Self::parse_track_directive(line)?;
            if let Some((number, audio)) = track_directive {
                if let Some(track) = current_track.take() {
                    tracks.push(track.finish(input)?);
                }
                if !audio {
                    if tracks.is_empty() {
                        return Self::failure(input, nom::error::ErrorKind::Tag);
                    }
                    break;
                }
                if current_file.is_none() {
                    return Self::failure(input, nom::error::ErrorKind::Tag);
                }
                current_track = Some(PendingCueTrack::new(number));
                continue;
            }

            let file_directive = Self::parse_file_directive(line)?;
            if let Some(file) = file_directive {
                current_file = Some(file);
                continue;
            }

            match current_track.as_mut() {
                Some(track) => {
                    let file_reference = current_file
                        .as_deref()
                        .expect("CUE TRACK parsing starts only after FILE is known");
                    Self::apply_track_line(input, line, track, file_reference)?;
                }
                None => {
                    Self::apply_header_line(
                        line,
                        &mut title,
                        &mut performer,
                        &mut catalog,
                        &mut date,
                    );
                }
            }
        }

        if let Some(track) = current_track {
            tracks.push(track.finish(input)?);
        }
        if tracks.is_empty() {
            return Err(nom::Err::Failure(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Many1,
            )));
        }

        // Some rippers (EAC v0.99pb4) write bogus INDEX 00 values that are
        // before the previous track's INDEX 01. Clear these so audio_start_cue_frames()
        // and pregap_duration_ms() don't use them downstream.
        for i in 1..tracks.len() {
            if let Some(pregap) = tracks[i].pregap_cue_frames {
                if pregap <= tracks[i - 1].start_cue_frames {
                    tracks[i].pregap_cue_frames = None;
                }
            }
        }
        for i in 0..tracks.len() {
            if i + 1 < tracks.len() {
                let next_track = &tracks[i + 1];
                let boundary = next_track
                    .pregap_cue_frames
                    .unwrap_or(next_track.start_cue_frames);
                tracks[i].end_cue_frames = Some(boundary);
            }
        }
        Ok((
            "",
            CueSheet {
                title,
                performer,
                catalog,
                date,
                tracks,
            },
        ))
    }

    fn apply_header_line(
        line: &str,
        title: &mut Option<String>,
        performer: &mut Option<String>,
        catalog: &mut Option<String>,
        date: &mut Option<String>,
    ) {
        match Self::keyword_and_rest(line) {
            Some(("REM", rest)) => {
                if let Some((keyword, value)) = Self::keyword_and_rest(rest) {
                    if keyword == "DATE" {
                        *date = Some(Self::strip_optional_quotes(value).to_string());
                    }
                }
            }
            Some(("CATALOG", rest)) => {
                *catalog = Some(Self::strip_optional_quotes(rest).to_string());
            }
            Some(("TITLE", rest)) => {
                if let Some(value) = Self::quoted_value(rest) {
                    *title = Some(value.to_string());
                }
            }
            Some(("PERFORMER", rest)) => {
                if let Some(value) = Self::quoted_value(rest) {
                    *performer = Some(value.to_string());
                }
            }
            Some((keyword, _)) if Self::header_keyword_is_known_skipped(keyword) => {}
            _ => warn!("unrecognized CUE header line: {:?}", line),
        }
    }

    fn apply_track_line<'a>(
        input: &'a str,
        line: &str,
        track: &mut PendingCueTrack,
        current_file: &str,
    ) -> Result<(), nom::Err<nom::error::Error<&'a str>>> {
        match Self::keyword_and_rest(line) {
            Some(("REM", _)) => {}
            Some(("TITLE", rest)) => {
                if let Some(value) = Self::quoted_value(rest) {
                    track.title = Some(value.to_string());
                }
            }
            Some(("PERFORMER", rest)) => {
                if let Some(value) = Self::quoted_value(rest) {
                    track.performer = Some(value.to_string());
                }
            }
            Some(("ISRC", rest)) => {
                let code = rest.split_whitespace().next().ok_or_else(|| {
                    nom::Err::Failure(nom::error::Error::new(
                        input,
                        nom::error::ErrorKind::TakeTill1,
                    ))
                })?;
                track.isrc = Some(code.to_string());
            }
            Some(("INDEX", rest)) => {
                let (index_number, cue_frames) = Self::parse_index_values(input, rest)?;
                match index_number {
                    0 => track.pregap_cue_frames = Some(cue_frames),
                    1 => {
                        track.start = Some((cue_frames, current_file.to_string()));
                    }
                    _ => {}
                }
            }
            Some((keyword, _)) if Self::track_keyword_is_known_skipped(keyword) => {}
            _ => warn!(
                "unrecognized CUE line in track {}: {:?}",
                track.number, line
            ),
        }
        Ok(())
    }

    fn parse_track_directive(
        input: &str,
    ) -> Result<Option<(u32, bool)>, nom::Err<nom::error::Error<&str>>> {
        let Some(("TRACK", rest)) = Self::keyword_and_rest(input) else {
            return Ok(None);
        };
        let mut parts = rest.split_whitespace();
        let number = Self::parse_u32_token(input, parts.next())?;
        let mode = parts.next().ok_or_else(|| {
            nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Tag))
        })?;
        Ok(Some((number, mode == "AUDIO")))
    }

    fn parse_file_directive(
        input: &str,
    ) -> Result<Option<String>, nom::Err<nom::error::Error<&str>>> {
        let Some(("FILE", rest)) = Self::keyword_and_rest(input) else {
            return Ok(None);
        };
        Ok(Some(Self::parse_file_name_value(input, rest)?))
    }

    fn parse_index_values<'a>(
        input: &'a str,
        rest: &str,
    ) -> Result<(u32, u64), nom::Err<nom::error::Error<&'a str>>> {
        let mut parts = rest.split_whitespace();
        let index_number = Self::parse_u32_token(input, parts.next())?;
        let time = parts.next().ok_or_else(|| {
            nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
        })?;
        let (_, cue_frames) = Self::parse_time(time).map_err(|_| {
            nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
        })?;
        Ok((index_number, cue_frames))
    }

    fn parse_u32_token<'a>(
        input: &'a str,
        token: Option<&str>,
    ) -> Result<u32, nom::Err<nom::error::Error<&'a str>>> {
        token
            .ok_or_else(|| {
                nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
            })?
            .parse::<u32>()
            .map_err(|_| {
                nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
            })
    }

    /// Spec-known commands valid inside a TRACK body that we don't store.
    /// Recognized so they don't trigger the unknown-line warning.
    fn track_keyword_is_known_skipped(keyword: &str) -> bool {
        const TRACK_BODY_SKIPPED: &[&str] = &[
            "FLAGS",
            "PREGAP",
            "POSTGAP",
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
        TRACK_BODY_SKIPPED.contains(&keyword)
    }
    /// Spec-known global-section commands we don't store. CD-Text keywords
    /// can also appear at the global level; CDTEXTFILE references an
    /// external binary CD-Text file.
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
        if let Some(value) = Self::quoted_value(input) {
            value
        } else {
            input
        }
    }

    fn parse_file_name_value<'a>(
        input: &'a str,
        rest: &str,
    ) -> Result<String, nom::Err<nom::error::Error<&'a str>>> {
        let rest = rest.trim_start();
        if let Some(value) = Self::quoted_value(rest) {
            return Ok(value.trim().to_string());
        }
        let name = rest.split_whitespace().next().ok_or_else(|| {
            nom::Err::Failure(nom::error::Error::new(
                input,
                nom::error::ErrorKind::TakeTill1,
            ))
        })?;
        Ok(name.to_string())
    }

    /// Parse time in MM:SS:FF format and return total CUE frames (1/75th second).
    fn parse_time(input: &str) -> IResult<&str, u64> {
        let mut parts = input.splitn(3, ':');
        let minutes = Self::parse_u64_time_field(input, parts.next())?;
        let seconds = Self::parse_u64_time_field(input, parts.next())?;
        let frames_and_rest = parts.next().ok_or_else(|| {
            nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
        })?;
        let frame_end = frames_and_rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(frames_and_rest.len());
        if frame_end == 0 {
            return Self::error(input, nom::error::ErrorKind::Digit);
        }
        let frames = frames_and_rest[..frame_end].parse::<u64>().map_err(|_| {
            nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
        })?;
        let total_cue_frames = (minutes * 60 + seconds) * 75 + frames;
        Ok((&frames_and_rest[frame_end..], total_cue_frames))
    }

    fn parse_u64_time_field<'a>(
        input: &'a str,
        token: Option<&str>,
    ) -> Result<u64, nom::Err<nom::error::Error<&'a str>>> {
        token
            .ok_or_else(|| {
                nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
            })?
            .parse::<u64>()
            .map_err(|_| {
                nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
            })
    }

    fn error<T>(input: &str, kind: nom::error::ErrorKind) -> IResult<&str, T> {
        Err(nom::Err::Error(nom::error::Error::new(input, kind)))
    }

    fn failure<T>(input: &str, kind: nom::error::ErrorKind) -> IResult<&str, T> {
        Err(nom::Err::Failure(nom::error::Error::new(input, kind)))
    }
}
#[cfg(test)]
mod tests;
