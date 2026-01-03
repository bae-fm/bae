use nom::{
    branch::alt,
    bytes::complete::{tag, take_until},
    character::complete::{digit1, line_ending, space1},
    combinator::{map_res, opt},
    multi::many0,
    sequence::{preceded, terminated, tuple},
    IResult,
};
use std::fs;
use std::path::Path;
use thiserror::Error;
#[derive(Debug, Error)]
pub enum CueFlacError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("FLAC parsing error: {0}")]
    Flac(String),
    #[error("CUE parsing error: {0}")]
    CueParsing(String),
}
/// Represents a single track in a CUE sheet
#[derive(Debug, Clone)]
pub struct CueTrack {
    pub number: u32,
    pub title: String,
    pub performer: Option<String>,
    pub start_time_ms: u64,
    pub pregap_time_ms: Option<u64>,
    pub end_time_ms: Option<u64>,
}

impl CueTrack {
    /// Where audio bytes begin: INDEX 00 if pregap exists, else INDEX 01
    pub fn audio_start_ms(&self) -> u64 {
        self.pregap_time_ms.unwrap_or(self.start_time_ms)
    }

    /// Duration of audio bytes (None for last track)
    pub fn audio_duration_ms(&self) -> Option<u64> {
        self.end_time_ms.map(|end| end - self.audio_start_ms())
    }

    /// Pregap duration in ms (0 if no pregap)
    pub fn pregap_duration_ms(&self) -> u64 {
        self.pregap_time_ms
            .map(|pregap| self.start_time_ms - pregap)
            .unwrap_or(0)
    }
}

/// Represents a parsed CUE sheet
#[derive(Debug, Clone)]
pub struct CueSheet {
    pub title: String,
    pub performer: String,
    pub tracks: Vec<CueTrack>,
}
/// FLAC header information extracted from file
#[derive(Debug, Clone)]
pub struct FlacHeaders {
    pub headers: Vec<u8>,
}

/// FLAC file analysis results
#[derive(Debug, Clone)]
pub struct FlacInfo {
    pub sample_rate: u32,
    pub total_samples: u64,
    pub audio_data_start: u64,
    pub audio_data_end: u64,
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

/// A single seek point in FLAC seektable
#[derive(Debug, Clone, Copy)]
pub struct SeekPoint {
    pub sample_number: u64,
    pub stream_offset: u64,
}

/// Dense seektable built by scanning FLAC frames during import.
///
/// We build our own seektable rather than using the embedded one because:
/// - Embedded seektables often have ~10 second gaps between entries
/// - We need ~100ms precision for smooth seeking during playback (scrubbing)
/// - Track boundary positioning also benefits from frame-accurate offsets
#[derive(Debug, Clone)]
pub struct DenseSeektable {
    pub entries: Vec<SeekPoint>,
}
/// Represents a CUE/FLAC pair found during import
#[derive(Debug, Clone)]
pub struct CueFlacPair {
    pub flac_path: std::path::PathBuf,
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
        let mut flac_files = Vec::new();
        let mut cue_files = Vec::new();
        for path in file_paths {
            if let Some(extension) = path.extension() {
                match extension.to_str() {
                    Some("flac") => flac_files.push(path.clone()),
                    Some("cue") => cue_files.push(path.clone()),
                    _ => {}
                }
            }
        }
        for cue_path in cue_files {
            let cue_stem = cue_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            for flac_path in &flac_files {
                let flac_stem = flac_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if cue_stem == flac_stem {
                    pairs.push(CueFlacPair {
                        flac_path: flac_path.clone(),
                        cue_path: cue_path.clone(),
                    });
                    break;
                }
            }
        }
        Ok(pairs)
    }
    /// Extract FLAC headers from a FLAC file, omitting the embedded seektable.
    ///
    /// We skip the seektable because our extracted track byte ranges are smaller
    /// than the full file. The embedded seektable would point to invalid offsets,
    /// causing decoder errors when it tries to use them for error recovery.
    pub fn extract_flac_headers(flac_path: &Path) -> Result<FlacHeaders, CueFlacError> {
        let file_data = fs::read(flac_path)?;
        Self::extract_flac_headers_from_data(&file_data)
    }

    /// Extract FLAC headers from data, omitting the embedded seektable.
    fn extract_flac_headers_from_data(file_data: &[u8]) -> Result<FlacHeaders, CueFlacError> {
        if file_data.len() < 4 || &file_data[0..4] != b"fLaC" {
            return Err(CueFlacError::Flac("Invalid FLAC signature".to_string()));
        }

        let mut headers = Vec::with_capacity(8192);
        headers.extend_from_slice(&file_data[0..4]); // "fLaC" magic

        let mut pos = 4;
        let mut found_last = false;

        while !found_last && pos + 4 <= file_data.len() {
            let header_byte = file_data[pos];
            let is_last = (header_byte & 0x80) != 0;
            let block_type = header_byte & 0x7F;
            let block_size = u32::from_be_bytes([
                0,
                file_data[pos + 1],
                file_data[pos + 2],
                file_data[pos + 3],
            ]) as usize;

            if pos + 4 + block_size > file_data.len() {
                return Err(CueFlacError::Flac("Block extends beyond file".to_string()));
            }

            // Skip seektable (type 3) - it points to offsets in the full file
            if block_type == 3 {
                pos += 4 + block_size;
                continue;
            }

            // Write this block to headers
            headers.push(header_byte);
            headers.extend_from_slice(&file_data[pos + 1..pos + 4]); // size bytes
            headers.extend_from_slice(&file_data[pos + 4..pos + 4 + block_size]); // block data

            found_last = is_last;
            pos += 4 + block_size;
        }

        // If we skipped blocks after the last one, we need to mark the actual last block
        // Find the last block in our headers and set its is_last flag
        if !found_last && headers.len() > 4 {
            // Find the last metadata block header and set is_last
            let mut scan_pos = 4;
            let mut last_block_pos = 4;
            while scan_pos + 4 <= headers.len() {
                last_block_pos = scan_pos;
                let block_size = u32::from_be_bytes([
                    0,
                    headers[scan_pos + 1],
                    headers[scan_pos + 2],
                    headers[scan_pos + 3],
                ]) as usize;
                scan_pos += 4 + block_size;
            }
            if last_block_pos < headers.len() {
                headers[last_block_pos] |= 0x80; // Set is_last flag
            }
        }

        Ok(FlacHeaders { headers })
    }
    /// Analyze a FLAC file and extract metadata
    pub fn analyze_flac(flac_path: &Path) -> Result<FlacInfo, CueFlacError> {
        let file_data = fs::read(flac_path)?;
        Self::analyze_flac_data(&file_data)
    }

    /// Analyze FLAC data and extract metadata
    fn analyze_flac_data(file_data: &[u8]) -> Result<FlacInfo, CueFlacError> {
        if file_data.len() < 4 || &file_data[0..4] != b"fLaC" {
            return Err(CueFlacError::Flac("Invalid FLAC signature".to_string()));
        }

        let mut pos = 4;
        let mut sample_rate = 0u32;
        let mut total_samples = 0u64;
        let audio_data_start: u64;

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
                // Total samples: bits 108-143 (36 bits)
                total_samples = (((block[13] & 0x0F) as u64) << 32)
                    | ((block[14] as u64) << 24)
                    | ((block[15] as u64) << 16)
                    | ((block[16] as u64) << 8)
                    | (block[17] as u64);
            }

            pos += block_size;
            if is_last {
                audio_data_start = pos as u64;
                break;
            }
        }

        Ok(FlacInfo {
            sample_rate,
            total_samples,
            audio_data_start,
            audio_data_end: file_data.len() as u64,
        })
    }

    /// Build a dense seektable by scanning FLAC frames.
    ///
    /// Embedded FLAC seektables typically have ~10 second gaps between entries,
    /// which is too coarse for:
    /// 1. Smooth seeking during playback (user scrubbing through a track)
    /// 2. Accurate track boundary positioning for CUE/FLAC
    ///
    /// By scanning every frame (~93ms at 44.1kHz), we get the precision needed
    /// for both use cases. The seektable is stored in the DB and used at playback.
    /// Build a dense seektable by scanning FLAC frames using libFLAC.
    ///
    /// This uses the official FLAC library to iterate through all frames,
    /// which is more reliable than manual sync-code scanning because libFLAC
    /// properly validates frame headers and CRCs.
    pub fn build_dense_seektable(file_data: &[u8], _flac_info: &FlacInfo) -> DenseSeektable {
        match crate::flac_decoder::scan_flac_frames(file_data) {
            Ok(scan_result) => {
                let entries: Vec<SeekPoint> = scan_result
                    .seektable
                    .into_iter()
                    .map(|e| SeekPoint {
                        sample_number: e.sample_number,
                        stream_offset: e.byte_offset,
                    })
                    .collect();
                DenseSeektable { entries }
            }
            Err(e) => {
                tracing::warn!("libFLAC scan failed: {}, returning empty seektable", e);
                DenseSeektable {
                    entries: Vec::new(),
                }
            }
        }
    }

    /// Find the byte range for a track in a CUE/FLAC file.
    ///
    /// Returns (start_byte, end_byte, frame_offset_samples):
    /// - start_byte, end_byte: absolute byte positions in the file
    /// - frame_offset_samples: samples to skip from start of decoded audio to actual track start
    ///
    /// Due to FLAC frame alignment, start_byte is at a frame boundary which may be
    /// up to ~4096 samples before the track's actual start. The frame_offset_samples tells
    /// the decoder how many samples to skip when playing.
    pub fn find_track_byte_range(
        start_time_ms: u64,
        end_time_ms: Option<u64>,
        seektable: &[SeekPoint],
        sample_rate: u32,
        total_samples: u64,
        audio_data_start: u64,
        audio_data_end: u64,
    ) -> (i64, i64, i64) {
        if sample_rate == 0 {
            return (audio_data_start as i64, audio_data_end as i64, 0);
        }

        let start_sample = (start_time_ms * sample_rate as u64) / 1000;
        let end_sample = end_time_ms
            .map(|ms| (ms * sample_rate as u64) / 1000)
            .unwrap_or(total_samples);

        // Find start byte using seektable, tracking the frame's sample number
        let (start_byte, frame_sample) = if seektable.is_empty() {
            // Linear interpolation without seektable - no offset needed
            let audio_size = audio_data_end - audio_data_start;
            (
                audio_data_start + (start_sample * audio_size) / total_samples,
                start_sample,
            )
        } else {
            // Find the seek point at or before start_sample
            let mut best_offset = 0u64;
            let mut best_sample = 0u64;
            for sp in seektable.iter() {
                if sp.sample_number <= start_sample {
                    best_offset = sp.stream_offset;
                    best_sample = sp.sample_number;
                } else {
                    break;
                }
            }
            (audio_data_start + best_offset, best_sample)
        };

        // Calculate frame offset: how many samples into decoded audio the track actually starts
        let frame_offset_samples = (start_sample - frame_sample) as i64;

        // Find end byte using seektable - use "at or after" to include all audio
        let end_byte = if seektable.is_empty() {
            let audio_size = audio_data_end - audio_data_start;
            audio_data_start + (end_sample * audio_size) / total_samples
        } else {
            // Find the seek point at or after end_sample to include all audio up to end.
            // This may create byte overlap with next track, but FLAC frames are self-contained
            // and the decoder handles overlapping byte ranges correctly.
            let mut best_offset = audio_data_end - audio_data_start;
            for sp in seektable.iter().rev() {
                if sp.sample_number >= end_sample {
                    best_offset = sp.stream_offset;
                } else {
                    break;
                }
            }
            audio_data_start + best_offset
        };

        (start_byte as i64, end_byte as i64, frame_offset_samples)
    }

    /// Find where audio frames start in a FLAC file
    /// Parse a CUE sheet file
    pub fn parse_cue_sheet(cue_path: &Path) -> Result<CueSheet, CueFlacError> {
        use tracing::{debug, error};
        debug!("Attempting to parse CUE sheet: {:?}", cue_path);
        debug!("CUE path exists: {}", cue_path.exists());
        debug!("CUE path absolute: {:?}", cue_path.canonicalize().ok());
        let content = fs::read_to_string(cue_path).map_err(|e| {
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
    /// Parse CUE sheet content using nom
    fn parse_cue_content(input: &str) -> IResult<&str, CueSheet> {
        let (input, _) = many0(alt((
            line_ending,
            space1,
            Self::parse_comment_line,
            Self::parse_file_line,
        )))(input)?;
        let (input, (title, performer)) = alt((
            |i| {
                let (i, performer) = Self::parse_performer(i)?;
                let (i, title) = Self::parse_title(i)?;
                Ok((i, (title, performer)))
            },
            |i| {
                let (i, title) = Self::parse_title(i)?;
                let (i, performer) = Self::parse_performer(i)?;
                Ok((i, (title, performer)))
            },
        ))(input)?;
        let (input, _) = many0(alt((
            line_ending,
            space1,
            Self::parse_file_line,
            Self::parse_comment_line,
        )))(input)?;
        let (input, tracks) = Self::parse_tracks(input)?;
        let mut tracks_with_end_times = tracks;
        for i in 0..tracks_with_end_times.len() {
            if i + 1 < tracks_with_end_times.len() {
                let next_track = &tracks_with_end_times[i + 1];
                // Use pregap (INDEX 00) as boundary if present, otherwise INDEX 01
                let boundary = next_track
                    .pregap_time_ms
                    .unwrap_or(next_track.start_time_ms);
                tracks_with_end_times[i].end_time_ms = Some(boundary);
            }
        }
        Ok((
            input,
            CueSheet {
                title,
                performer,
                tracks: tracks_with_end_times,
            },
        ))
    }
    /// Parse and skip a REM (comment) line
    fn parse_comment_line(input: &str) -> IResult<&str, &str> {
        let (input, _) = tag("REM")(input)?;
        let (input, _) = take_until("\n")(input)?;
        let (input, _) = line_ending(input)?;
        Ok((input, ""))
    }
    /// Parse and skip a FILE line
    fn parse_file_line(input: &str) -> IResult<&str, &str> {
        let (input, _) = tag("FILE")(input)?;
        let (input, _) = take_until("\n")(input)?;
        let (input, _) = line_ending(input)?;
        Ok((input, ""))
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
    /// Parse all TRACK entries
    fn parse_tracks(input: &str) -> IResult<&str, Vec<CueTrack>> {
        many0(Self::parse_track)(input)
    }
    /// Parse a single TRACK entry
    fn parse_track(input: &str) -> IResult<&str, CueTrack> {
        let (input, _) = many0(alt((line_ending, space1, Self::parse_comment_line)))(input)?;
        let (input, _) = tag("TRACK")(input)?;
        let (input, _) = space1(input)?;
        let (input, number) = map_res(digit1, |s: &str| s.parse::<u32>())(input)?;
        let (input, _) = space1(input)?;
        let (input, _) = tag("AUDIO")(input)?;
        let (input, _) = opt(line_ending)(input)?;
        let (input, _) = many0(space1)(input)?;
        let (input, _) = tag("TITLE")(input)?;
        let (input, _) = space1(input)?;
        let (input, title) = Self::parse_quoted_string(input)?;
        let (input, _) = opt(line_ending)(input)?;
        let (input, performer) = opt(preceded(
            tuple((many0(space1), tag("PERFORMER"), space1)),
            terminated(Self::parse_quoted_string, opt(line_ending)),
        ))(input)?;
        let (input, pregap_time_ms) = opt(|input| {
            let (input, _) = many0(alt((line_ending, space1, Self::parse_comment_line)))(input)?;
            let (input, _) = tag("INDEX")(input)?;
            let (input, _) = space1(input)?;
            let (input, _) = tag("00")(input)?;
            let (input, _) = space1(input)?;
            let (input, pregap_ms) = Self::parse_time(input)?;
            let (input, _) = opt(line_ending)(input)?;
            Ok((input, pregap_ms))
        })(input)?;
        let (input, _) = many0(alt((line_ending, space1, Self::parse_comment_line)))(input)?;
        let (input, _) = tag("INDEX")(input)?;
        let (input, _) = space1(input)?;
        let (input, _) = tag("01")(input)?;
        let (input, _) = space1(input)?;
        let (input, start_time_ms) = Self::parse_time(input)?;
        let (input, _) = opt(line_ending)(input)?;
        Ok((
            input,
            CueTrack {
                number,
                title,
                performer,
                start_time_ms,
                pregap_time_ms,
                end_time_ms: None,
            },
        ))
    }
    /// Parse quoted string
    fn parse_quoted_string(input: &str) -> IResult<&str, String> {
        let (input, _) = tag("\"")(input)?;
        let (input, content) = take_until("\"")(input)?;
        let (input, _) = tag("\"")(input)?;
        Ok((input, content.to_string()))
    }
    /// Parse time in MM:SS:FF format and convert to milliseconds
    fn parse_time(input: &str) -> IResult<&str, u64> {
        let (input, minutes) = map_res(digit1, |s: &str| s.parse::<u64>())(input)?;
        let (input, _) = tag(":")(input)?;
        let (input, seconds) = map_res(digit1, |s: &str| s.parse::<u64>())(input)?;
        let (input, _) = tag(":")(input)?;
        let (input, frames) = map_res(digit1, |s: &str| s.parse::<u64>())(input)?;
        let total_ms = (minutes * 60 * 1000) + (seconds * 1000) + (frames * 1000 / 75);
        Ok((input, total_ms))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_time() {
        let result = CueFlacProcessor::parse_time("03:45:12");
        assert!(result.is_ok());
        let (_, time_ms) = result.unwrap();
        assert_eq!(time_ms, 225160);
    }
    #[test]
    fn test_parse_time_zero() {
        let result = CueFlacProcessor::parse_time("00:00:00");
        assert!(result.is_ok());
        let (_, time_ms) = result.unwrap();
        assert_eq!(time_ms, 0);
    }
    #[test]
    fn test_parse_time_large_values() {
        let result = CueFlacProcessor::parse_time("60:35:00");
        assert!(result.is_ok());
        let (_, time_ms) = result.unwrap();
        assert_eq!(time_ms, 60 * 60 * 1000 + 35 * 1000);
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
        let input = "FILE \"Artist Name - Album Title.flac\" WAVE\n";
        let result = CueFlacProcessor::parse_file_line(input);
        assert!(result.is_ok());
        let (remaining, _) = result.unwrap();
        assert_eq!(remaining, "");
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
        assert_eq!(cue_sheet.title, "Test Album");
        assert_eq!(cue_sheet.performer, "Test Artist");
        assert_eq!(cue_sheet.tracks.len(), 2);
        assert_eq!(cue_sheet.tracks[0].title, "Track 1");
        assert_eq!(cue_sheet.tracks[0].start_time_ms, 0);
        assert_eq!(cue_sheet.tracks[1].title, "Track 2");
        assert_eq!(cue_sheet.tracks[1].start_time_ms, 3 * 60 * 1000 + 45 * 1000);
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
        assert_eq!(cue_sheet.title, "Album Title");
        assert_eq!(cue_sheet.performer, "Artist Name");
        assert_eq!(cue_sheet.tracks.len(), 2);
        assert_eq!(cue_sheet.tracks[0].title, "Track One");
        assert_eq!(cue_sheet.tracks[1].title, "Track Two");
    }
    #[test]
    fn test_parse_cue_sheet_with_windows_line_endings() {
        let cue_content = "REM GENRE \"Genre Name\"\r\nPERFORMER \"Test Artist\"\r\nTITLE \"Test Album\"\r\nFILE \"test.flac\" WAVE\r\n  TRACK 01 AUDIO\r\n    TITLE \"Track 1\"\r\n    PERFORMER \"Test Artist\"\r\n    INDEX 01 00:00:00\r\n";
        let result = CueFlacProcessor::parse_cue_content(cue_content);
        assert!(result.is_ok());
        let (_, cue_sheet) = result.unwrap();
        assert_eq!(cue_sheet.title, "Test Album");
        assert_eq!(cue_sheet.performer, "Test Artist");
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
        assert_eq!(cue_sheet.tracks[0].end_time_ms, Some(3 * 60 * 1000));
        assert_eq!(cue_sheet.tracks[1].end_time_ms, Some(6 * 60 * 1000));
        assert_eq!(cue_sheet.tracks[2].end_time_ms, None);
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
        let cue_content = r#"PERFORMER "Led Zeppelin"
TITLE "Led Zeppelin I"
FILE "Led Zeppelin I.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Good Times Bad Times"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Babe I'm Gonna Leave You"
    INDEX 00 02:46:00
    INDEX 01 02:49:00
  TRACK 03 AUDIO
    TITLE "You Shook Me"
    INDEX 01 09:31:00
"#;
        let result = CueFlacProcessor::parse_cue_content(cue_content);
        assert!(result.is_ok());
        let (_, cue_sheet) = result.unwrap();

        // Track 1 should end at track 2's pregap (INDEX 00), not INDEX 01
        let track1_end_ms = cue_sheet.tracks[0].end_time_ms.unwrap();
        let track2_pregap_ms = cue_sheet.tracks[1].pregap_time_ms.unwrap();
        let track2_start_ms = cue_sheet.tracks[1].start_time_ms;

        // Verify pregap was parsed correctly (INDEX 00 = 2:46 = 166000ms)
        assert_eq!(track2_pregap_ms, 2 * 60 * 1000 + 46 * 1000);
        // Verify start was parsed correctly (INDEX 01 = 2:49 = 169000ms)
        assert_eq!(track2_start_ms, 2 * 60 * 1000 + 49 * 1000);

        // THE KEY ASSERTION: Track 1 ends at pregap, not at start
        assert_eq!(
            track1_end_ms, track2_pregap_ms,
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
        let pregap_ms = track2.pregap_time_ms.unwrap();
        let start_ms = track2.start_time_ms;

        // Pregap duration should be 3 seconds (3:03 - 3:00 = 3000ms)
        let pregap_duration = start_ms - pregap_ms;
        assert_eq!(pregap_duration, 3000, "Pregap duration should be 3 seconds");
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
        let track2_end_ms = cue_sheet.tracks[1].end_time_ms.unwrap();
        let track3_start_ms = cue_sheet.tracks[2].start_time_ms;

        assert_eq!(
            track2_end_ms, track3_start_ms,
            "Track 2 should end at track 3's INDEX 01 (no pregap)"
        );
        assert_eq!(track2_end_ms, 6 * 60 * 1000);
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
        assert_eq!(track1.pregap_duration_ms(), 0);
        // Track 1 ends at track 2's pregap (2:46 = 166000ms)
        assert_eq!(track1.audio_duration_ms(), Some(166000));

        // Track 2: has pregap
        let track2 = &cue_sheet.tracks[1];
        assert_eq!(track2.audio_start_ms(), 166000); // 2:46 (INDEX 00)
        assert_eq!(track2.pregap_duration_ms(), 3000); // 3 seconds
                                                       // Duration: 9:31 - 2:46 = 405 seconds (includes pregap)
        assert_eq!(track2.audio_duration_ms(), Some(405000));

        // Track 3: last track, no end time
        let track3 = &cue_sheet.tracks[2];
        assert_eq!(track3.audio_start_ms(), 571000); // 9:31
        assert_eq!(track3.pregap_duration_ms(), 0);
        assert_eq!(track3.audio_duration_ms(), None);
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
        assert_eq!(cue_sheet.title, "Test Album");
        assert_eq!(cue_sheet.performer, "Test Artist");
        assert_eq!(cue_sheet.tracks.len(), 3, "Should parse 3 tracks");
        assert_eq!(cue_sheet.tracks[0].title, "Track 1");
        assert_eq!(cue_sheet.tracks[1].title, "Track 2");
        assert_eq!(cue_sheet.tracks[2].title, "Track 3 With Multiple Sections");
        assert_eq!(cue_sheet.tracks[0].start_time_ms, 0);
        assert_eq!(
            cue_sheet.tracks[1].start_time_ms,
            6 * 60 * 1000 + 17 * 1000 + 53 * 1000 / 75,
        );
    }

    #[test]
    fn test_consecutive_tracks_have_no_gaps_in_byte_ranges() {
        // Two consecutive tracks sharing a boundary must not have gaps.
        // Overlap is OK (FLAC decoder handles it), but gaps lose audio.
        let sample_rate = 44100u32;
        let total_samples = 44100 * 300; // 5 minutes
        let audio_data_start = 1000u64;
        let audio_data_end = 10_000_000u64;

        // Seektable with entries every ~10 seconds
        let seektable: Vec<SeekPoint> = (0..30)
            .map(|i| SeekPoint {
                sample_number: i * 44100 * 10, // every 10 sec
                stream_offset: i * 300_000,    // ~300KB per 10 sec
            })
            .collect();

        // Boundary at 2:46 (166 seconds) = sample 7,318,600
        let boundary_ms = 166_000u64;

        // Track 1: 0 to boundary
        let (_, track1_end, _) = CueFlacProcessor::find_track_byte_range(
            0,
            Some(boundary_ms),
            &seektable,
            sample_rate,
            total_samples,
            audio_data_start,
            audio_data_end,
        );

        // Track 2: boundary to end
        let (track2_start, _, _) = CueFlacProcessor::find_track_byte_range(
            boundary_ms,
            None,
            &seektable,
            sample_rate,
            total_samples,
            audio_data_start,
            audio_data_end,
        );

        assert!(
            track1_end >= track2_start,
            "Track 1 end ({}) must be >= track 2 start ({}) - gaps would lose audio!",
            track1_end,
            track2_start
        );
    }

    #[test]
    fn test_dense_seektable_gives_accurate_byte_offsets() {
        // Simulate a dense seektable with ~93ms precision (4096 samples per frame)
        // For a 45-minute file at 44100Hz
        let sample_rate = 44100u32;
        let samples_per_frame = 4096u32;
        let total_duration_s = 45 * 60; // 45 minutes
        let total_samples = sample_rate as u64 * total_duration_s;
        let audio_data_start = 100_000u64; // 100KB of headers
        let bytes_per_frame = 15_000u64; // ~15KB per frame (~163KB/s for CD quality)
        let num_frames = total_samples / samples_per_frame as u64;
        let audio_data_end = audio_data_start + num_frames * bytes_per_frame;

        // Build a dense seektable (one entry per frame)
        let seektable: Vec<SeekPoint> = (0..num_frames)
            .map(|i| SeekPoint {
                sample_number: i * samples_per_frame as u64,
                stream_offset: i * bytes_per_frame,
            })
            .collect();

        // Track 2 starts at 02:47:02 (INDEX 01) = 167027ms
        let track2_start_ms = 167027u64;
        let _track2_start_sample = track2_start_ms * sample_rate as u64 / 1000; // ~7,365,991

        let (start_byte, _, _) = CueFlacProcessor::find_track_byte_range(
            track2_start_ms,
            None,
            &seektable,
            sample_rate,
            total_samples,
            audio_data_start,
            audio_data_end,
        );

        // The byte offset should correspond to a sample number close to track2_start_sample
        // Find which seektable entry we landed on
        let frame_index = (start_byte as u64 - audio_data_start) / bytes_per_frame;
        let frame_sample = frame_index * samples_per_frame as u64;
        let frame_time_ms = frame_sample * 1000 / sample_rate as u64;

        // The difference should be at most one frame (~93ms)
        let diff_ms = track2_start_ms as i64 - frame_time_ms as i64;
        assert!(
            (0..=93).contains(&diff_ms),
            "Byte offset should be within 93ms of track start. Got frame at {}ms, wanted {}ms, diff={}ms",
            frame_time_ms, track2_start_ms, diff_ms
        );
    }
}
