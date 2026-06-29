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
mod tests;
