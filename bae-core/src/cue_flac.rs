use nom::{
    branch::alt,
    bytes::complete::{tag, take_until},
    character::complete::{digit1, line_ending, space1},
    combinator::{map_res, opt},
    multi::many0,
    IResult,
};
use std::fs;
use std::path::Path;
use thiserror::Error;
use tracing::warn;
#[derive(Debug, Error)]
pub enum CueFlacError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("FLAC parsing error: {0}")]
    Flac(String),
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

/// Classifies a `REM <keyword> <value>` line. `Other` covers REM keywords we
/// don't capture (e.g. REM COMMENT, REM GENRE, REM DISCID, ripper-specific
/// extensions).
#[derive(Debug)]
enum RemKind {
    Date(String),
    Other,
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
/// FLAC file analysis results
#[derive(Debug, Clone)]
pub struct FlacInfo {
    pub sample_rate: u32,
    pub bits_per_sample: u32,
    pub channels: u32,
    pub total_samples: u64,
}

impl FlacInfo {
    /// Calculate duration in milliseconds
    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }
        (self.total_samples * 1000) / self.sample_rate as u64
    }
}

/// Represents a CUE/FLAC pair found during import
#[derive(Debug, Clone)]
pub struct CueFlacPair {
    pub audio_path: std::path::PathBuf,
    pub cue_path: std::path::PathBuf,
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
                    Some("flac") | Some("ape") | Some("m4a") => audio_files.push(path.clone()),
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
    /// Analyze a FLAC file and extract metadata
    pub fn analyze_flac(flac_path: &Path) -> Result<FlacInfo, CueFlacError> {
        let file_data = fs::read(flac_path)?;
        Self::analyze_flac_data(&file_data)
    }

    /// Analyze in-memory FLAC data and extract metadata.
    pub fn analyze_flac_data(file_data: &[u8]) -> Result<FlacInfo, CueFlacError> {
        if file_data.len() < 4 || &file_data[0..4] != b"fLaC" {
            return Err(CueFlacError::Flac("Invalid FLAC signature".to_string()));
        }

        let mut pos = 4;
        let mut sample_rate = 0u32;
        let mut bits_per_sample = 0u32;
        let mut channels = 0u32;
        let mut total_samples = 0u64;

        loop {
            if pos + 4 > file_data.len() {
                return Err(CueFlacError::Flac("Unexpected end of file".to_string()));
            }

            let header = u32::from_be_bytes([
                file_data[pos],
                file_data[pos + 1],
                file_data[pos + 2],
                file_data[pos + 3],
            ]);

            let is_last = (header & 0x80000000) != 0;
            let block_type = ((header >> 24) & 0x7F) as u8;
            let block_size = (header & 0x00FFFFFF) as usize;
            pos += 4;

            if pos + block_size > file_data.len() {
                return Err(CueFlacError::Flac("Block extends beyond file".to_string()));
            }

            if block_type == 0 && block_size >= 18 {
                // STREAMINFO block
                let block = &file_data[pos..pos + block_size];
                // Sample rate: bits 80-99 (20 bits)
                sample_rate = ((block[10] as u32) << 12)
                    | ((block[11] as u32) << 4)
                    | ((block[12] as u32) >> 4);
                // Channels - 1: bits 100-102 (3 bits)
                channels = (((block[12] >> 1) & 0x07) as u32) + 1;
                // Bits per sample - 1: bits 103-107 (5 bits, spans bytes 12-13)
                bits_per_sample =
                    ((((block[12] & 0x01) as u32) << 4) | (((block[13] & 0xF0) >> 4) as u32)) + 1;
                // Total samples: bits 108-143 (36 bits)
                total_samples = (((block[13] & 0x0F) as u64) << 32)
                    | ((block[14] as u64) << 24)
                    | ((block[15] as u64) << 16)
                    | ((block[16] as u64) << 8)
                    | (block[17] as u64);
            }

            pos += block_size;
            if is_last {
                break;
            }
        }

        if bits_per_sample == 0 {
            return Err(CueFlacError::Flac(
                "Could not determine bits per sample from FLAC".to_string(),
            ));
        }

        Ok(FlacInfo {
            sample_rate,
            bits_per_sample,
            channels,
            total_samples,
        })
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
        let mut input = input;

        loop {
            let stripped = input.trim_start();
            if stripped.is_empty() {
                break;
            }
            if Self::starts_with_keyword(stripped, "TRACK") {
                break;
            }
            if let Ok((i, _)) = line_ending::<_, nom::error::Error<&str>>(input) {
                input = i;
                continue;
            }
            if let Ok((i, _)) = space1::<_, nom::error::Error<&str>>(input) {
                input = i;
                continue;
            }
            if let Ok((i, name)) = Self::parse_file_line(input) {
                current_file = Some(name);
                input = i;
                continue;
            }
            if let Ok((i, kind)) = Self::parse_rem_classified(input) {
                match kind {
                    RemKind::Date(d) => date = Some(d),
                    RemKind::Other => {}
                }
                input = i;
                continue;
            }
            if let Ok((i, c)) = Self::parse_catalog_line(input) {
                catalog = Some(c);
                input = i;
                continue;
            }
            if let Ok((i, t)) = Self::parse_title(input) {
                title = Some(t);
                input = i;
                continue;
            }
            if let Ok((i, p)) = Self::parse_performer(input) {
                performer = Some(p);
                input = i;
                continue;
            }
            if Self::header_starts_with_known_skipped(stripped) {
                input = Self::consume_line(input);
                continue;
            }
            let line = Self::peek_line(stripped);
            warn!("unrecognized CUE header line: {:?}", line);
            input = Self::consume_line(input);
        }

        // Body: TRACK entries that all read and possibly update
        // `current_file`. A FILE that appears between tracks shifts every
        // subsequent track to the new file; a FILE that appears inside a
        // track body (between INDEX 00 and INDEX 01 — the per-track-rip
        // convention) shifts only the rest of that body. Each track's
        // `file_reference` is whichever FILE was current at the moment its
        // INDEX 01 was parsed.
        let mut current_file = current_file.ok_or_else(|| {
            // TRACK before any FILE: no audio file to bind to.
            nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Tag))
        })?;
        let mut tracks: Vec<CueTrack> = Vec::new();
        loop {
            let stripped = input.trim_start();
            if stripped.is_empty() {
                break;
            }
            // Trailing non-AUDIO content (e.g. MODE1/2048 data track after
            // the final AUDIO track) terminates the body once we've parsed
            // at least one AUDIO track. `parse_track` itself fails at the
            // `tag("AUDIO")` step for non-AUDIO modes — that's a nom Error
            // (not Failure), so we stop the loop rather than abort the
            // parse. Failures from inside a track body (no INDEX 01,
            // malformed INDEX, etc.) propagate.
            match Self::parse_track(input, &mut current_file) {
                Ok((rest, track)) => {
                    tracks.push(track);
                    input = rest;
                }
                Err(nom::Err::Error(_)) => break,
                Err(e) => return Err(e),
            }
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
            input,
            CueSheet {
                title,
                performer,
                catalog,
                date,
                tracks,
            },
        ))
    }
    /// Parse and skip a REM (comment) line.
    fn parse_comment_line(input: &str) -> IResult<&str, &str> {
        let (input, _) = tag("REM")(input)?;
        let (input, _) = take_until("\n")(input)?;
        let (input, _) = line_ending(input)?;
        Ok((input, ""))
    }
    /// Parse a REM line and classify the keyword. Returns `RemKind::Other`
    /// for REM keywords we don't capture (e.g. REM COMMENT, ripper-specific
    /// extensions); the line is still consumed.
    fn parse_rem_classified(input: &str) -> IResult<&str, RemKind> {
        let (input, _) = tag("REM")(input)?;
        let (input, _) = space1(input)?;
        let kw_end = input
            .find(|c: char| c.is_whitespace())
            .unwrap_or(input.len());
        let keyword = &input[..kw_end];
        let after_kw = &input[kw_end..];
        let value_start = after_kw.trim_start_matches([' ', '\t']);
        let line_end = value_start.find('\n').unwrap_or(value_start.len());
        let raw_value = value_start[..line_end].trim_end_matches('\r').trim();
        let unquoted = match raw_value
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
        {
            Some(s) => s,
            None => raw_value,
        };
        let kind = match keyword {
            "DATE" => RemKind::Date(unquoted.to_string()),
            _ => RemKind::Other,
        };
        let after_value = &value_start[line_end..];
        let (after_value, _) = opt(line_ending)(after_value)?;
        Ok((after_value, kind))
    }
    /// Parse a FILE line, returning the referenced filename.
    /// Handles both quoted (`FILE "foo.flac" WAVE`) and unquoted forms.
    fn parse_file_line(input: &str) -> IResult<&str, String> {
        let (input, _) = tag("FILE")(input)?;
        let (input, _) = space1(input)?;
        let (input, name) = Self::parse_file_name(input)?;
        let (input, _) = take_until("\n")(input)?;
        let (input, _) = line_ending(input)?;
        Ok((input, name))
    }

    fn parse_file_name(input: &str) -> IResult<&str, String> {
        if let Ok((rest, quoted)) = Self::parse_quoted_string(input) {
            return Ok((rest, quoted.trim().to_string()));
        }
        let end = input.find(char::is_whitespace).unwrap_or(input.len());
        if end == 0 {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::TakeTill1,
            )));
        }
        Ok((&input[end..], input[..end].to_string()))
    }
    /// Parse a CATALOG line, returning the catalog number (UPC/EAN/MCN).
    fn parse_catalog_line(input: &str) -> IResult<&str, String> {
        let (input, _) = tag("CATALOG")(input)?;
        let (input, _) = space1(input)?;
        let line_end = input.find('\n').unwrap_or(input.len());
        let raw = input[..line_end].trim_end_matches('\r').trim();
        let value = match raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            Some(s) => s.to_string(),
            None => raw.to_string(),
        };
        let after = &input[line_end..];
        let (after, _) = opt(line_ending)(after)?;
        Ok((after, value))
    }
    /// Parse TITLE line
    fn parse_title(input: &str) -> IResult<&str, String> {
        let (input, _) = many0(alt((line_ending, space1, Self::parse_comment_line)))(input)?;
        let (input, _) = tag("TITLE")(input)?;
        let (input, _) = space1(input)?;
        let (input, title) = Self::parse_quoted_string(input)?;
        let (input, _) = opt(line_ending)(input)?;
        Ok((input, title))
    }
    /// Parse PERFORMER line
    fn parse_performer(input: &str) -> IResult<&str, String> {
        let (input, _) = many0(alt((line_ending, space1, Self::parse_comment_line)))(input)?;
        let (input, _) = tag("PERFORMER")(input)?;
        let (input, _) = space1(input)?;
        let (input, performer) = Self::parse_quoted_string(input)?;
        let (input, _) = opt(line_ending)(input)?;
        Ok((input, performer))
    }
    /// Parse a single TRACK entry as a per-line classifier. `current_file`
    /// is read-write state shared with the caller — the track's
    /// `file_reference` is whichever value `current_file` held when INDEX 01
    /// was parsed (the file where the track actually starts), and any FILE
    /// directive encountered inside the body updates `current_file` for the
    /// rest of this track and every track that follows.
    ///
    /// Only AUDIO tracks are parsed; non-AUDIO modes (MODE1/2048, MODE2/2352,
    /// etc.) cause `parse_track` to fail with a nom Error so the caller's
    /// loop terminates. Spec-known commands within a track body can appear
    /// in any order (TITLE, PERFORMER, ISRC, FLAGS, PREGAP, INDEX 00–99,
    /// POSTGAP, plus CD-Text keywords). FILE may also appear inside the
    /// body (the per-track-rip convention puts a track's INDEX 00 at the
    /// end of file N and its INDEX 01 at the start of file N+1, with FILE
    /// sitting between the two INDEX lines). Unknown commands (typos,
    /// vendor extensions) emit a warning and are skipped. The track ends
    /// at the next TRACK keyword or at EOF; INDEX 01 is required.
    fn parse_track<'a>(input: &'a str, current_file: &mut String) -> IResult<&'a str, CueTrack> {
        let (input, _) = many0(alt((line_ending, space1, Self::parse_comment_line)))(input)?;
        let (input, _) = tag("TRACK")(input)?;
        let (input, _) = space1(input)?;
        let (input, number) = map_res(digit1, |s: &str| s.parse::<u32>())(input)?;
        let (input, _) = space1(input)?;
        let (input, _) = tag("AUDIO")(input)?;
        let (input, _) = opt(take_until("\n"))(input)?;
        let (mut input, _) = opt(line_ending)(input)?;

        let mut title: Option<String> = None;
        let mut performer: Option<String> = None;
        let mut isrc: Option<String> = None;
        let mut pregap_cue_frames: Option<u64> = None;
        let mut start: Option<(u64, String)> = None;

        loop {
            let stripped = input.trim_start();
            if stripped.is_empty() {
                break;
            }
            if Self::starts_with_keyword(stripped, "TRACK") {
                break;
            }
            if let Ok((i, _)) = line_ending::<_, nom::error::Error<&str>>(input) {
                input = i;
                continue;
            }
            if let Ok((i, _)) = space1::<_, nom::error::Error<&str>>(input) {
                input = i;
                continue;
            }
            if let Ok((i, _)) = Self::parse_comment_line(input) {
                input = i;
                continue;
            }
            if let Ok((i, name)) = Self::parse_file_line(input) {
                *current_file = name;
                input = i;
                continue;
            }
            if let Ok((i, t)) = Self::parse_title(input) {
                title = Some(t);
                input = i;
                continue;
            }
            if let Ok((i, p)) = Self::parse_performer(input) {
                performer = Some(p);
                input = i;
                continue;
            }
            if let Ok((i, code)) = Self::parse_isrc_line(input) {
                isrc = Some(code);
                input = i;
                continue;
            }
            if let Ok((i, (idx_num, frames))) = Self::parse_index_line(input) {
                match idx_num {
                    0 => pregap_cue_frames = Some(frames),
                    1 => start = Some((frames, current_file.clone())),
                    _ => {}
                }
                input = i;
                continue;
            }
            if Self::track_starts_with_known_skipped(stripped) {
                input = Self::consume_line(input);
                continue;
            }
            let line = Self::peek_line(stripped);
            warn!("unrecognized CUE line in track {}: {:?}", number, line);
            input = Self::consume_line(input);
        }

        let (start_cue_frames, file_reference) = start.ok_or_else(|| {
            nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Tag))
        })?;

        Ok((
            input,
            CueTrack {
                number,
                title,
                performer,
                isrc,
                file_reference,
                start_cue_frames,
                pregap_cue_frames,
                end_cue_frames: None,
            },
        ))
    }
    /// Parse `ISRC <code>` and return the code (first whitespace-delimited
    /// token after `ISRC`). Per spec the code is 12 alphanumeric characters
    /// (CCXXXYYNNNNN); we don't validate format here, just extract.
    fn parse_isrc_line(input: &str) -> IResult<&str, String> {
        let (input, _) = many0(alt((line_ending, space1, Self::parse_comment_line)))(input)?;
        let (input, _) = tag("ISRC")(input)?;
        let (input, _) = space1(input)?;
        let (input, code) = nom::bytes::complete::take_till1(|c: char| c.is_whitespace())(input)?;
        let code = code.to_string();
        let (input, _) = opt(take_until("\n"))(input)?;
        let (input, _) = opt(line_ending)(input)?;
        Ok((input, code))
    }
    /// Parse `INDEX <n> mm:ss:ff` and return the index number and frames.
    fn parse_index_line(input: &str) -> IResult<&str, (u32, u64)> {
        let (input, _) = many0(alt((line_ending, space1, Self::parse_comment_line)))(input)?;
        let (input, _) = tag("INDEX")(input)?;
        let (input, _) = space1(input)?;
        let (input, idx_num) = map_res(digit1, |s: &str| s.parse::<u32>())(input)?;
        let (input, _) = space1(input)?;
        let (input, frames) = Self::parse_time(input)?;
        let (input, _) = opt(line_ending)(input)?;
        Ok((input, (idx_num, frames)))
    }
    /// Spec-known commands valid inside a TRACK body that we don't store.
    /// Recognized so they don't trigger the unknown-line warning.
    fn track_starts_with_known_skipped(input: &str) -> bool {
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
        TRACK_BODY_SKIPPED
            .iter()
            .any(|kw| Self::starts_with_keyword(input, kw))
    }
    /// Spec-known global-section commands we don't store. CD-Text keywords
    /// can also appear at the global level; CDTEXTFILE references an
    /// external binary CD-Text file.
    fn header_starts_with_known_skipped(input: &str) -> bool {
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
        HEADER_SKIPPED
            .iter()
            .any(|kw| Self::starts_with_keyword(input, kw))
    }
    /// Whether `input` begins with `keyword` followed by whitespace or EOF
    /// (so that "TRACKBALL" doesn't match keyword "TRACK").
    fn starts_with_keyword(input: &str, keyword: &str) -> bool {
        if !input.starts_with(keyword) {
            return false;
        }
        match input[keyword.len()..].chars().next() {
            None => true,
            Some(c) => c.is_whitespace(),
        }
    }
    /// Advance past the next line ending. If no line ending, returns "".
    fn consume_line(input: &str) -> &str {
        match input.find('\n') {
            Some(idx) => &input[idx + 1..],
            None => "",
        }
    }
    /// Return the next line of input without trailing CR/LF, without consuming.
    fn peek_line(input: &str) -> &str {
        let end = input.find('\n').unwrap_or(input.len());
        let line = &input[..end];
        line.strip_suffix('\r').unwrap_or(line)
    }
    /// Parse quoted string
    fn parse_quoted_string(input: &str) -> IResult<&str, String> {
        let (input, _) = tag("\"")(input)?;
        let (input, content) = take_until("\"")(input)?;
        let (input, _) = tag("\"")(input)?;
        Ok((input, content.to_string()))
    }
    /// Parse time in MM:SS:FF format and return total CUE frames (1/75th second).
    fn parse_time(input: &str) -> IResult<&str, u64> {
        let (input, minutes) = map_res(digit1, |s: &str| s.parse::<u64>())(input)?;
        let (input, _) = tag(":")(input)?;
        let (input, seconds) = map_res(digit1, |s: &str| s.parse::<u64>())(input)?;
        let (input, _) = tag(":")(input)?;
        let (input, frames) = map_res(digit1, |s: &str| s.parse::<u64>())(input)?;
        let total_cue_frames = (minutes * 60 + seconds) * 75 + frames;
        Ok((input, total_cue_frames))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_time() {
        // "MM:SS:FF" -> total CUE frames (75 frames per second).
        let cases = [
            ("03:45:12", (3 * 60 + 45) * 75 + 12),
            ("00:00:00", 0),
            ("60:35:00", (60 * 60 + 35) * 75),
        ];
        for (input, expected) in cases {
            let (_, cue_frames) = CueFlacProcessor::parse_time(input).unwrap();
            assert_eq!(cue_frames, expected, "input: {input}");
        }
    }
    #[test]
    fn test_parse_quoted_string() {
        let result = CueFlacProcessor::parse_quoted_string("\"Test Album\"");
        assert!(result.is_ok());
        let (_, string) = result.unwrap();
        assert_eq!(string, "Test Album");
    }
    #[test]
    fn test_parse_quoted_string_with_special_chars() {
        let result = CueFlacProcessor::parse_quoted_string(
            "\"Track with Sections: i. First Part / ii. Second Part / iii. Third Part\"",
        );
        assert!(result.is_ok());
        let (_, string) = result.unwrap();
        assert_eq!(
            string,
            "Track with Sections: i. First Part / ii. Second Part / iii. Third Part",
        );
    }
    #[test]
    fn test_parse_comment_line() {
        let input = "REM GENRE \"Genre Name\"\n";
        let result = CueFlacProcessor::parse_comment_line(input);
        assert!(result.is_ok());
        let (remaining, _) = result.unwrap();
        assert_eq!(remaining, "");
    }
    #[test]
    fn test_parse_file_line() {
        // FILE lines: a quoted name keeps its spaces; an unquoted name stops at
        // the first whitespace.
        let cases = [
            (
                "FILE \"Artist Name - Album Title.flac\" WAVE\n",
                "Artist Name - Album Title.flac",
            ),
            ("FILE album.ape WAVE\n", "album.ape"),
        ];
        for (input, expected) in cases {
            let (remaining, name) = CueFlacProcessor::parse_file_line(input).unwrap();
            assert_eq!(remaining, "", "input: {input}");
            assert_eq!(name, expected, "input: {input}");
        }
    }
    #[test]
    fn parse_multi_file_cue_stamps_per_track_file_reference() {
        // One FILE per TRACK, the spec shape used by lossy-format rips
        // (rippers don't concatenate per-track files to avoid re-encoding).
        let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "01 - Track One.m4a" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Artist Name"
    INDEX 01 00:00:00
FILE "02 - Track Two.m4a" WAVE
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 01 00:00:00
FILE "03 - Track Three.m4a" WAVE
  TRACK 03 AUDIO
    TITLE "Track Three"
    INDEX 01 00:00:00
"#;
        let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
        assert_eq!(sheet.tracks.len(), 3);
        assert_eq!(sheet.tracks[0].file_reference, "01 - Track One.m4a");
        assert_eq!(sheet.tracks[1].file_reference, "02 - Track Two.m4a");
        assert_eq!(sheet.tracks[2].file_reference, "03 - Track Three.m4a");
        assert!(sheet.single_file().is_none());
    }

    #[test]
    fn parse_single_file_cue_stamps_same_reference_on_every_track() {
        let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 01 03:45:00
  TRACK 03 AUDIO
    INDEX 01 07:30:00
"#;
        let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
        assert_eq!(sheet.tracks.len(), 3);
        for track in &sheet.tracks {
            assert_eq!(track.file_reference, "Album.flac");
        }
        assert_eq!(sheet.single_file(), Some("Album.flac"));
    }

    #[test]
    fn parse_multi_file_cue_splits_at_file_inside_track_body() {
        // PERFORMER/TITLE/INDEX sit inside the track body; the FILE that
        // starts the next group terminates the current track.
        let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "01.m4a" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Artist Name"
    INDEX 01 00:00:00
FILE "02.m4a" WAVE
  TRACK 02 AUDIO
    TITLE "Track Two"
    PERFORMER "Artist Name"
    INDEX 01 00:00:00
"#;
        let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
        assert_eq!(sheet.tracks.len(), 2);
        assert_eq!(sheet.tracks[0].title.as_deref(), Some("Track One"));
        assert_eq!(sheet.tracks[0].file_reference, "01.m4a");
        assert_eq!(sheet.tracks[1].title.as_deref(), Some("Track Two"));
        assert_eq!(sheet.tracks[1].file_reference, "02.m4a");
    }

    #[test]
    fn parse_per_track_rip_with_file_intruding_mid_track_body() {
        // Per-track rip convention: each track "owns" the pregap-tail of the
        // previous file. Track 2's INDEX 00 (pregap start) sits at the end of
        // file 1; FILE for file 2 appears mid-track-body; track 2's INDEX 01
        // (start) sits at the beginning of file 2. The track's
        // `file_reference` must be the file where INDEX 01 lives (the one
        // we'd actually play), not the file where INDEX 00 lives.
        let cue_content = r#"TITLE "Album Title"
PERFORMER "Artist Name"
FILE "01.wav" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 00 02:55:33
FILE "02.wav" WAVE
    INDEX 01 00:00:00
  TRACK 03 AUDIO
    TITLE "Track Three"
    INDEX 00 02:17:46
FILE "03.wav" WAVE
    INDEX 01 00:00:00
"#;
        let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
        assert_eq!(sheet.tracks.len(), 3);
        assert_eq!(sheet.tracks[0].file_reference, "01.wav");
        assert_eq!(sheet.tracks[0].title.as_deref(), Some("Track One"));
        // Track 2 has pregap in file 1 (INDEX 00 at 2:55:33), start in
        // file 2 (INDEX 01 at 00:00:00). `file_reference` is the start file.
        assert_eq!(sheet.tracks[1].file_reference, "02.wav");
        assert_eq!(sheet.tracks[1].title.as_deref(), Some("Track Two"));
        assert_eq!(
            sheet.tracks[1].pregap_cue_frames,
            Some((2 * 60 + 55) * 75 + 33),
        );
        assert_eq!(sheet.tracks[1].start_cue_frames, 0);
        // Track 3 same shape: pregap in file 2, start in file 3.
        assert_eq!(sheet.tracks[2].file_reference, "03.wav");
        assert_eq!(sheet.tracks[2].title.as_deref(), Some("Track Three"));
        assert_eq!(
            sheet.tracks[2].pregap_cue_frames,
            Some((2 * 60 + 17) * 75 + 46),
        );
        assert_eq!(sheet.tracks[2].start_cue_frames, 0);
        // Multi-FILE — no single playback file.
        assert!(sheet.single_file().is_none());
    }

    #[test]
    fn parse_second_track_without_index_01_propagates_failure() {
        // When a track body lacks INDEX 01, the body-loop's error arm must
        // propagate `Err::Failure` instead of treating it as a loop
        // terminator — otherwise the parse returns `Ok` with whatever
        // tracks accumulated so far and the remainder is silently dropped.
        let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
"#;
        let err = CueFlacProcessor::parse_cue_content(cue_content).unwrap_err();
        assert!(
            matches!(err, nom::Err::Failure(_)),
            "expected Failure when a track body lacks INDEX 01, got {err:?}",
        );
    }

    #[test]
    fn parse_cue_content_rejects_track_before_any_file() {
        // A TRACK with no FILE above it has no audio file to bind to. The
        // empty file_reference would silently mislead downstream pair
        // detection; reject at parse time.
        let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
"#;
        let err = CueFlacProcessor::parse_cue_content(cue_content).unwrap_err();
        assert!(
            matches!(err, nom::Err::Failure(_)),
            "expected Failure for TRACK before FILE, got {err:?}",
        );
    }

    #[test]
    fn test_parse_simple_cue_sheet() {
        let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    PERFORMER "Test Artist"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    PERFORMER "Test Artist"
    INDEX 01 03:45:00
"#;
        let result = CueFlacProcessor::parse_cue_content(cue_content);
        assert!(result.is_ok());
        let (_, cue_sheet) = result.unwrap();
        assert_eq!(cue_sheet.title.as_deref(), Some("Test Album"));
        assert_eq!(cue_sheet.performer.as_deref(), Some("Test Artist"));
        assert_eq!(cue_sheet.tracks.len(), 2);
        assert_eq!(cue_sheet.tracks[0].title.as_deref(), Some("Track 1"));
        assert_eq!(cue_sheet.tracks[0].start_time_ms(), 0);
        assert_eq!(cue_sheet.tracks[1].title.as_deref(), Some("Track 2"));
        assert_eq!(
            cue_sheet.tracks[1].start_time_ms(),
            3 * 60 * 1000 + 45 * 1000
        );
    }
    #[test]
    fn test_parse_cue_sheet_with_catalog_line() {
        let cue_content = r#"REM GENRE Rock
REM DATE 1989
REM DISCID 8F09030C
REM COMMENT "ExactAudioCopy v1.0b3"
CATALOG 0000000000000
PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Artist Name"
    INDEX 00 00:00:00
    INDEX 01 00:00:37
  TRACK 02 AUDIO
    TITLE "Track Two"
    PERFORMER "Artist Name"
    INDEX 00 03:39:16
    INDEX 01 03:40:62
  TRACK 03 AUDIO
    TITLE "Track Three"
    PERFORMER "Artist Name"
    INDEX 00 05:44:73
    INDEX 01 05:46:50
"#;
        let result = CueFlacProcessor::parse_cue_content(cue_content);
        assert!(
            result.is_ok(),
            "Failed to parse CUE with CATALOG line: {:?}",
            result.err()
        );
        let (_, cue_sheet) = result.unwrap();
        assert_eq!(cue_sheet.title.as_deref(), Some("Album Title"));
        assert_eq!(cue_sheet.performer.as_deref(), Some("Artist Name"));
        assert_eq!(cue_sheet.tracks.len(), 3);
        assert_eq!(cue_sheet.tracks[0].title.as_deref(), Some("Track One"));
        // Track 1 has pregap at 00:00:00, INDEX 01 at 00:00:37
        assert_eq!(cue_sheet.tracks[0].pregap_cue_frames, Some(0));
        assert_eq!(cue_sheet.tracks[0].start_cue_frames, 37);
        // Track 2 has pregap
        assert!(cue_sheet.tracks[1].pregap_cue_frames.is_some());
    }

    #[test]
    fn test_parse_cue_sheet_with_comments() {
        let cue_content = r#"REM GENRE "Genre Name"
REM DATE 2000 / 2004
REM COMMENT "Vinyl Rip by User Name"
PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Artist Name"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    PERFORMER "Artist Name"
    INDEX 01 03:04:00
"#;
        let result = CueFlacProcessor::parse_cue_content(cue_content);
        assert!(result.is_ok());
        let (_, cue_sheet) = result.unwrap();
        assert_eq!(cue_sheet.title.as_deref(), Some("Album Title"));
        assert_eq!(cue_sheet.performer.as_deref(), Some("Artist Name"));
        assert_eq!(cue_sheet.tracks.len(), 2);
        assert_eq!(cue_sheet.tracks[0].title.as_deref(), Some("Track One"));
        assert_eq!(cue_sheet.tracks[1].title.as_deref(), Some("Track Two"));
    }
    #[test]
    fn test_parse_cue_sheet_with_windows_line_endings() {
        let cue_content = "REM GENRE \"Genre Name\"\r\nPERFORMER \"Test Artist\"\r\nTITLE \"Test Album\"\r\nFILE \"test.flac\" WAVE\r\n  TRACK 01 AUDIO\r\n    TITLE \"Track 1\"\r\n    PERFORMER \"Test Artist\"\r\n    INDEX 01 00:00:00\r\n";
        let result = CueFlacProcessor::parse_cue_content(cue_content);
        assert!(result.is_ok());
        let (_, cue_sheet) = result.unwrap();
        assert_eq!(cue_sheet.title.as_deref(), Some("Test Album"));
        assert_eq!(cue_sheet.performer.as_deref(), Some("Test Artist"));
        assert_eq!(cue_sheet.tracks.len(), 1);
    }
    #[test]
    fn test_parse_cue_sheet_calculates_end_times() {
        let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    PERFORMER "Test Artist"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    PERFORMER "Test Artist"
    INDEX 01 03:00:00
  TRACK 03 AUDIO
    TITLE "Track 3"
    PERFORMER "Test Artist"
    INDEX 01 06:00:00
"#;
        let result = CueFlacProcessor::parse_cue_content(cue_content);
        assert!(result.is_ok());
        let (_, cue_sheet) = result.unwrap();
        assert_eq!(cue_sheet.tracks[0].end_time_ms(), Some(3 * 60 * 1000));
        assert_eq!(cue_sheet.tracks[1].end_time_ms(), Some(6 * 60 * 1000));
        assert_eq!(cue_sheet.tracks[2].end_time_ms(), None);
    }
    #[test]
    fn test_parse_cue_sheet_without_per_track_performer() {
        let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    INDEX 01 03:00:00
"#;
        let result = CueFlacProcessor::parse_cue_content(cue_content);
        assert!(result.is_ok());
        let (_, cue_sheet) = result.unwrap();
        assert_eq!(cue_sheet.tracks.len(), 2);
        assert_eq!(cue_sheet.tracks[0].performer, None);
        assert_eq!(cue_sheet.tracks[1].performer, None);
    }
    #[test]
    fn parse_cue_content_captures_release_identification_signals() {
        let cue_content = r#"REM GENRE Rock
REM DATE 2001
REM DISCID 7F0A4C0B
REM COMMENT "Some Ripper v1.0"
CATALOG 0123456789012
PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#;
        let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
        assert_eq!(sheet.catalog.as_deref(), Some("0123456789012"));
        assert_eq!(sheet.date.as_deref(), Some("2001"));
    }

    #[test]
    fn parse_rem_strips_quoted_values_and_ignores_unknown_keywords() {
        let cue_content = r#"REM DATE "2000 / 2004"
REM GENRE "Indie Rock"
REM DISCID 7F0A4C0B
REM CUSTOM_RIPPER_FIELD some-value
PERFORMER "Artist Name"
TITLE "Album Title"
FILE "x.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#;
        let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
        assert_eq!(sheet.date.as_deref(), Some("2000 / 2004"));
        // REM GENRE, REM DISCID, and unknown REM keywords are silently consumed;
        // only the identification fields the parser captures are populated.
        assert_eq!(sheet.catalog, None);
    }

    #[test]
    fn parse_track_with_isrc_populates_field() {
        let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Artist Name"
    ISRC GB000000000001
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    PERFORMER "Artist Name"
    ISRC GB000000000002
    INDEX 01 03:45:00
"#;
        let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
        assert_eq!(sheet.tracks.len(), 2);
        assert_eq!(sheet.tracks[0].isrc.as_deref(), Some("GB000000000001"));
        assert_eq!(sheet.tracks[1].isrc.as_deref(), Some("GB000000000002"));
    }

    #[test]
    fn parse_track_silently_consumes_spec_known_commands() {
        let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Artist Name"
    SONGWRITER "Songwriter Name"
    FLAGS DCP
    PREGAP 00:02:00
    INDEX 01 00:00:00
    POSTGAP 00:01:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    FLAGS PRE 4CH
    INDEX 01 03:45:00
"#;
        let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
        assert_eq!(sheet.tracks.len(), 2);
        assert_eq!(sheet.tracks[0].title.as_deref(), Some("Track One"));
        assert_eq!(sheet.tracks[1].title.as_deref(), Some("Track Two"));
    }

    #[test]
    fn parse_track_skips_unknown_command_without_failing() {
        let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    SOMETHING_UNKNOWN value
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    VENDOR_EXTENSION whatever
    INDEX 01 03:45:00
"#;
        let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
        assert_eq!(sheet.tracks.len(), 2);
        assert_eq!(sheet.tracks[0].title.as_deref(), Some("Track One"));
        assert_eq!(sheet.tracks[1].title.as_deref(), Some("Track Two"));
    }

    #[test]
    fn parse_track_without_index_01_returns_err() {
        let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Artist Name"
"#;
        assert!(CueFlacProcessor::parse_cue_content(cue_content).is_err());
    }

    #[test]
    fn parse_zero_track_cue_returns_err() {
        let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
"#;
        assert!(CueFlacProcessor::parse_cue_content(cue_content).is_err());
    }

    #[test]
    fn parse_track_consumes_subindexes_above_one() {
        let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
    INDEX 02 01:30:00
    INDEX 03 02:15:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 01 03:45:00
"#;
        let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
        assert_eq!(sheet.tracks.len(), 2);
        assert_eq!(sheet.tracks[0].start_cue_frames, 0);
        assert_eq!(sheet.tracks[1].start_cue_frames, 3 * 60 * 75 + 45 * 75);
    }

    #[test]
    fn test_parse_cue_with_index_00_minimal_repro() {
        let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    INDEX 00 03:00:00
    INDEX 01 03:01:00
"#;
        let result = CueFlacProcessor::parse_cue_content(cue_content);
        assert!(result.is_ok());
        let (_, cue_sheet) = result.unwrap();
        assert_eq!(cue_sheet.tracks.len(), 2, "Should parse 2 tracks");
    }

    #[test]
    fn test_pregap_sets_correct_track_boundary() {
        // Track 2 has a 3-second pregap (INDEX 00 at 2:46, INDEX 01 at 2:49)
        // Track 1 should end at INDEX 00 (2:46), not INDEX 01 (2:49)
        let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "Test Artist - Test Album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 00 02:46:00
    INDEX 01 02:49:00
  TRACK 03 AUDIO
    TITLE "Track Three"
    INDEX 01 09:31:00
"#;
        let result = CueFlacProcessor::parse_cue_content(cue_content);
        assert!(result.is_ok());
        let (_, cue_sheet) = result.unwrap();

        // Track 1 should end at track 2's pregap (INDEX 00), not INDEX 01
        let track1_end_ms = cue_sheet.tracks[0].end_time_ms().unwrap();
        let track2_pregap_frames = cue_sheet.tracks[1].pregap_cue_frames.unwrap();
        let track2_start_frames = cue_sheet.tracks[1].start_cue_frames;

        // Verify pregap was parsed correctly (INDEX 00 = 2:46:00 = 12450 CUE frames)
        assert_eq!(track2_pregap_frames, (2 * 60 + 46) * 75);
        // Verify start was parsed correctly (INDEX 01 = 2:49:00 = 12675 CUE frames)
        assert_eq!(track2_start_frames, (2 * 60 + 49) * 75);

        // THE KEY ASSERTION: Track 1 ends at pregap, not at start
        assert_eq!(
            track1_end_ms,
            track2_pregap_frames * 1000 / 75,
            "Track 1 should end at track 2's INDEX 00 (pregap), not INDEX 01"
        );
    }

    #[test]
    fn test_pregap_duration_calculation() {
        // Pregap duration = INDEX 01 - INDEX 00
        let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    INDEX 00 03:00:00
    INDEX 01 03:03:00
"#;
        let result = CueFlacProcessor::parse_cue_content(cue_content);
        assert!(result.is_ok());
        let (_, cue_sheet) = result.unwrap();

        let track2 = &cue_sheet.tracks[1];
        let pregap_frames = track2.pregap_cue_frames.unwrap();
        let start_frames = track2.start_cue_frames;

        // Pregap duration should be 3 seconds (3:03 - 3:00 = 225 CUE frames = 3s)
        let pregap_duration_frames = start_frames - pregap_frames;
        assert_eq!(
            pregap_duration_frames,
            3 * 75,
            "Pregap duration should be 3 seconds"
        );
    }

    #[test]
    fn test_track_without_pregap_uses_start_for_boundary() {
        // Track 3 has no pregap, so track 2 should end at track 3's INDEX 01
        let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    INDEX 00 03:00:00
    INDEX 01 03:02:00
  TRACK 03 AUDIO
    TITLE "Track 3"
    INDEX 01 06:00:00
"#;
        let result = CueFlacProcessor::parse_cue_content(cue_content);
        assert!(result.is_ok());
        let (_, cue_sheet) = result.unwrap();

        // Track 2 should end at track 3's start (no pregap on track 3)
        let track2_end_frames = cue_sheet.tracks[1].end_cue_frames.unwrap();
        let track3_start_frames = cue_sheet.tracks[2].start_cue_frames;

        assert_eq!(
            track2_end_frames, track3_start_frames,
            "Track 2 should end at track 3's INDEX 01 (no pregap)"
        );
        assert_eq!(track2_end_frames, 6 * 60 * 75);
    }

    #[test]
    fn test_cue_track_audio_methods() {
        // Track 2 has pregap at 2:46 (INDEX 00) and start at 2:49 (INDEX 01)
        // Track 3 starts at 9:31, so track 2 ends at 9:31
        let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    INDEX 00 02:46:00
    INDEX 01 02:49:00
  TRACK 03 AUDIO
    TITLE "Track 3"
    INDEX 01 09:31:00
"#;
        let (_, cue_sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();

        // Track 1: no pregap
        let track1 = &cue_sheet.tracks[0];
        assert_eq!(track1.audio_start_ms(), 0);
        assert_eq!(track1.pregap_duration_ms(), None);

        // Track 2: has pregap
        let track2 = &cue_sheet.tracks[1];
        assert_eq!(track2.audio_start_ms(), 166000); // 2:46 (INDEX 00)
        assert_eq!(track2.pregap_duration_ms(), Some(3000)); // 3 seconds
                                                             // Track duration excludes pregap: 9:31 - 2:49 = 402 seconds
        assert_eq!(track2.track_duration_ms(), Some(402000));

        // Track 3: last track, no end time
        let track3 = &cue_sheet.tracks[2];
        assert_eq!(track3.audio_start_ms(), 571000); // 9:31
        assert_eq!(track3.pregap_duration_ms(), None);
        assert_eq!(track3.track_duration_ms(), None);
    }

    #[test]
    fn test_parse_cue_with_rem_between_title_and_file() {
        let cue_content = r#"REM DATE 1970
REM DISCID A1B2C3D4
REM COMMENT "ExactAudioCopy v1.3"
PERFORMER "Test Artist"
TITLE "Test Album"
REM COMPOSER ""
FILE "Test Artist - Test Album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    PERFORMER "Test Artist"
    REM COMPOSER ""
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    PERFORMER "Test Artist"
    REM COMPOSER ""
    INDEX 01 06:17:53
  TRACK 03 AUDIO
    TITLE "Track 3 With Multiple Sections"
    PERFORMER "Test Artist"
    REM COMPOSER ""
    INDEX 00 10:39:50
    INDEX 01 10:41:28
"#;
        let result = CueFlacProcessor::parse_cue_content(cue_content);
        assert!(
            result.is_ok(),
            "Should parse CUE with REM between TITLE and FILE"
        );
        let (_, cue_sheet) = result.unwrap();
        assert_eq!(cue_sheet.title.as_deref(), Some("Test Album"));
        assert_eq!(cue_sheet.performer.as_deref(), Some("Test Artist"));
        assert_eq!(cue_sheet.tracks.len(), 3, "Should parse 3 tracks");
        assert_eq!(cue_sheet.tracks[0].title.as_deref(), Some("Track 1"));
        assert_eq!(cue_sheet.tracks[1].title.as_deref(), Some("Track 2"));
        assert_eq!(
            cue_sheet.tracks[2].title.as_deref(),
            Some("Track 3 With Multiple Sections")
        );
        assert_eq!(cue_sheet.tracks[0].start_time_ms(), 0);
        assert_eq!(
            cue_sheet.tracks[1].start_time_ms(),
            6 * 60 * 1000 + 17 * 1000 + 53 * 1000 / 75,
        );
    }

    #[test]
    fn test_bogus_pregap_before_previous_track_uses_index01() {
        // Real-world CUE: EAC wrote INDEX 00 05:05:00 for all tracks 3-19,
        // which is before track 2's INDEX 01. This caused:
        //   - Track 2 duration = 0 (boundary == start)
        //   - Track 3+ duration underflow (boundary < start on u64)
        let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.ape" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 00 00:00:00
    INDEX 01 00:00:32
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 01 05:05:00
  TRACK 03 AUDIO
    TITLE "Track Three"
    INDEX 00 05:05:00
    INDEX 01 08:31:20
  TRACK 04 AUDIO
    TITLE "Track Four"
    INDEX 00 05:05:00
    INDEX 01 11:01:30
"#;
        let (_, cue_sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();

        // Track 1: end should be track 2's start (no pregap on track 2)
        let track1 = &cue_sheet.tracks[0];
        assert_eq!(track1.start_time_ms(), 426); // INDEX 01 00:00:32 = 32 frames = 426.67ms
        let track1_dur = track1.track_duration_ms().unwrap();
        assert!(
            track1_dur > 300_000,
            "Track 1 should be ~5min, got {}ms",
            track1_dur
        );

        // Track 2: end should NOT use track 3's bogus pregap (05:05:00 = same as track 2 start)
        // It should use track 3's INDEX 01 instead
        let track2 = &cue_sheet.tracks[1];
        assert_eq!(track2.start_time_ms(), 305_000); // 5:05:00
        let track2_dur = track2.track_duration_ms().unwrap();
        assert!(
            track2_dur > 200_000,
            "Track 2 should be ~3.5min, got {}ms",
            track2_dur
        );

        // Track 3: end should use track 4's INDEX 01 (not bogus pregap)
        let track3 = &cue_sheet.tracks[2];
        assert_eq!(track3.start_time_ms(), 511_266); // 8:31:20
        let track3_dur = track3.track_duration_ms().unwrap();
        assert!(
            track3_dur > 140_000,
            "Track 3 should be ~2.5min, got {}ms",
            track3_dur
        );

        // audio_start_ms must ignore bogus pregap and use INDEX 01
        assert_eq!(
            track3.audio_start_ms(),
            track3.start_time_ms(),
            "Track 3 audio_start_ms should use INDEX 01, not bogus INDEX 00"
        );
        assert_eq!(
            track3.pregap_duration_ms(),
            None,
            "Bogus pregap should be cleared, leaving no duration"
        );
    }

    #[test]
    fn test_detect_cue_flac_from_paths_lowercase() {
        use std::path::PathBuf;

        let paths = vec![
            PathBuf::from("/music/album.flac"),
            PathBuf::from("/music/album.cue"),
            PathBuf::from("/music/cover.jpg"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].audio_path, PathBuf::from("/music/album.flac"));
        assert_eq!(pairs[0].cue_path, PathBuf::from("/music/album.cue"));
    }

    #[test]
    fn test_detect_cue_flac_from_paths_uppercase() {
        use std::path::PathBuf;

        let paths = vec![
            PathBuf::from("/music/album.FLAC"),
            PathBuf::from("/music/album.CUE"),
            PathBuf::from("/music/cover.jpg"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

        assert_eq!(
            pairs.len(),
            1,
            "Should detect CUE/FLAC pair with uppercase extensions"
        );
        assert_eq!(pairs[0].audio_path, PathBuf::from("/music/album.FLAC"));
        assert_eq!(pairs[0].cue_path, PathBuf::from("/music/album.CUE"));
    }

    #[test]
    fn test_detect_cue_flac_from_paths_mixed_case() {
        use std::path::PathBuf;

        let paths = vec![
            PathBuf::from("/music/album.Flac"),
            PathBuf::from("/music/album.Cue"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

        assert_eq!(
            pairs.len(),
            1,
            "Should detect CUE/FLAC pair with mixed case extensions"
        );
    }

    #[test]
    fn test_detect_cue_flac_from_paths_no_match() {
        use std::path::PathBuf;

        let paths = vec![
            PathBuf::from("/music/album.flac"),
            PathBuf::from("/music/different.cue"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

        assert_eq!(
            pairs.len(),
            0,
            "Should not match CUE/FLAC with different stems"
        );
    }

    #[test]
    fn test_detect_cue_flac_from_paths_multiple_pairs() {
        use std::path::PathBuf;

        let paths = vec![
            PathBuf::from("/music/disc1.flac"),
            PathBuf::from("/music/disc1.cue"),
            PathBuf::from("/music/disc2.flac"),
            PathBuf::from("/music/disc2.cue"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

        assert_eq!(pairs.len(), 2, "Should detect multiple CUE/FLAC pairs");
    }

    #[test]
    fn test_detect_cue_flac_from_paths_with_spaces_and_dashes() {
        use std::path::PathBuf;

        let paths = vec![
            PathBuf::from("/music/Some Artist - Some Album.cue"),
            PathBuf::from("/music/Some Artist - Some Album.flac"),
            PathBuf::from("/music/front.jpg"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

        assert_eq!(
            pairs.len(),
            1,
            "Should detect CUE/FLAC pair with spaces and dashes in filename"
        );
        assert!(pairs[0]
            .audio_path
            .to_string_lossy()
            .contains("Some Artist"));
        assert!(pairs[0].cue_path.to_string_lossy().contains("Some Artist"));
    }

    #[test]
    fn test_detect_cue_ape_pair() {
        use std::path::PathBuf;

        let paths = vec![
            PathBuf::from("/music/album.ape"),
            PathBuf::from("/music/album.cue"),
            PathBuf::from("/music/cover.jpg"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].audio_path, PathBuf::from("/music/album.ape"));
        assert_eq!(pairs[0].cue_path, PathBuf::from("/music/album.cue"));
    }

    #[test]
    fn test_detect_cue_ape_uppercase() {
        use std::path::PathBuf;

        let paths = vec![
            PathBuf::from("/music/album.APE"),
            PathBuf::from("/music/album.CUE"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].audio_path, PathBuf::from("/music/album.APE"));
    }

    #[test]
    fn test_detect_cue_ape_no_match_different_stems() {
        use std::path::PathBuf;

        let paths = vec![
            PathBuf::from("/music/album.ape"),
            PathBuf::from("/music/different.cue"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();
        assert_eq!(pairs.len(), 0);
    }

    #[test]
    fn test_detect_cue_alac_pair() {
        use std::path::PathBuf;

        // `.m4a` is the MP4 container extension for CUE+ALAC rips. Detection
        // is extension-only at this layer; the codec is confirmed later by
        // the analyzer.
        let paths = vec![
            PathBuf::from("/music/album.m4a"),
            PathBuf::from("/music/album.cue"),
            PathBuf::from("/music/cover.jpg"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].audio_path, PathBuf::from("/music/album.m4a"));
        assert_eq!(pairs[0].cue_path, PathBuf::from("/music/album.cue"));
    }

    #[test]
    fn test_detect_cue_alac_uppercase() {
        use std::path::PathBuf;

        let paths = vec![
            PathBuf::from("/music/album.M4A"),
            PathBuf::from("/music/album.CUE"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].audio_path, PathBuf::from("/music/album.M4A"));
    }

    #[test]
    fn test_detect_mixed_flac_and_ape_pairs() {
        use std::path::PathBuf;

        let paths = vec![
            PathBuf::from("/music/disc1.flac"),
            PathBuf::from("/music/disc1.cue"),
            PathBuf::from("/music/disc2.ape"),
            PathBuf::from("/music/disc2.cue"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].audio_path, PathBuf::from("/music/disc1.flac"));
        assert_eq!(pairs[1].audio_path, PathBuf::from("/music/disc2.ape"));
    }

    #[test]
    fn test_detect_multi_disc_same_stem_different_dirs() {
        use std::path::PathBuf;

        let paths = vec![
            PathBuf::from("/music/CD1/CDImage.ape"),
            PathBuf::from("/music/CD1/CDImage.cue"),
            PathBuf::from("/music/CD2/CDImage.ape"),
            PathBuf::from("/music/CD2/CDImage.cue"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].cue_path, PathBuf::from("/music/CD1/CDImage.cue"));
        assert_eq!(pairs[0].audio_path, PathBuf::from("/music/CD1/CDImage.ape"));
        assert_eq!(pairs[1].cue_path, PathBuf::from("/music/CD2/CDImage.cue"));
        assert_eq!(pairs[1].audio_path, PathBuf::from("/music/CD2/CDImage.ape"));
    }

    #[test]
    fn test_detect_three_discs_same_stem() {
        use std::path::PathBuf;

        let paths = vec![
            PathBuf::from("/music/CD1/CDImage.flac"),
            PathBuf::from("/music/CD1/CDImage.cue"),
            PathBuf::from("/music/CD2/CDImage.flac"),
            PathBuf::from("/music/CD2/CDImage.cue"),
            PathBuf::from("/music/CD3/CDImage.flac"),
            PathBuf::from("/music/CD3/CDImage.cue"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

        assert_eq!(pairs.len(), 3);
        assert_eq!(
            pairs[0].audio_path,
            PathBuf::from("/music/CD1/CDImage.flac")
        );
        assert_eq!(
            pairs[1].audio_path,
            PathBuf::from("/music/CD2/CDImage.flac")
        );
        assert_eq!(
            pairs[2].audio_path,
            PathBuf::from("/music/CD3/CDImage.flac")
        );
    }

    #[test]
    fn test_detect_cue_and_audio_must_be_same_directory() {
        use std::path::PathBuf;

        // CUE in one dir, audio in another — not a valid pair
        let paths = vec![
            PathBuf::from("/music/CDImage.cue"),
            PathBuf::from("/music/CD1/CDImage.ape"),
        ];

        let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

        assert_eq!(pairs.len(), 0);
    }
}
