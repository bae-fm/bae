//! What a rip log says about the ripped bits.
//!
//! EAC, XLD and CUERipper each look the disc up in AccurateRip — EAC 1.x and
//! CUERipper also in the CUETools database — and print, per track, how many
//! other people's copies of that disc carry the same audio. That count is the
//! only claim in the log that survives the rip: the CRCs are ours, the
//! confidence is everybody else's.
//!
//! The text arrives decoded (`crate::text_encoding::read_text_file` handles
//! the UTF-16LE-with-BOM that EAC writes by default).
//!
//! Rather than one parser per ripper, each line shape is recognized on its
//! own. The rippers share more than they differ — CUERipper 2.1.5 writes an
//! EAC-shaped body under its own banner, and the CUETools block is identical
//! under EAC and CUERipper — so splitting by banner would duplicate every
//! shared block and misread the logs that mix them. The banner names the
//! ripper; the line shapes carry the results.

use std::collections::BTreeMap;

/// A rip log's verification content. Everything else in the log — drive,
/// offsets, settings, file names — is outside what a release is verified by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RipLog {
    pub ripper: Ripper,
    pub tracks: Vec<TrackResult>,
    pub accuraterip: Option<AccurateRipSummary>,
    pub ctdb: Option<CtdbSummary>,
}

/// The program that wrote the log, from its banner line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ripper {
    Eac { version: String },
    Xld { version: String },
    CueRipper { version: String },
    Unknown,
}

/// One track's row in the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackResult {
    pub number: u32,
    /// The CRC of the verification pass, when the rip made one.
    pub test_crc: Option<u32>,
    /// The CRC of the audio that was kept.
    pub copy_crc: Option<u32>,
    pub accuraterip: AccurateRipTrack,
    pub ctdb: Option<CtdbTrack>,
}

/// What AccurateRip said about one track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccurateRipTrack {
    /// `confidence` other copies of this disc carry the same audio. The `crc`
    /// is this rip's AccurateRip signature, which only the logs that print one
    /// per track carry — a summary row states the count and nothing else.
    Matched {
        confidence: u32,
        version: ArVersion,
        crc: Option<u32>,
    },
    /// The database holds copies of this track and none of them agree with the
    /// ripped bits. EAC names the confidence and CRC of the copy it compared
    /// against; XLD names neither, so both stay absent rather than being
    /// guessed from the signatures it printed earlier in the block.
    Mismatch {
        confidence: Option<u32>,
        crc: Option<u32>,
        database_crc: Option<u32>,
    },
    /// The database holds no copy of this track to compare against.
    NotInDatabase,
    /// The log carries the track but says nothing about AccurateRip: the rip
    /// never asked, or asked and the answer is not in this file.
    NotChecked,
}

/// Which of AccurateRip's two checksum schemes matched. Rips from before the
/// v2 scheme, and EAC 0.99, print no version at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArVersion {
    V1,
    V2,
    Both,
    Unknown,
}

/// What the CUETools database said about one track. `total` is how many copies
/// of the disc it holds and `confidence` how many of those agree on the audio,
/// which on a match is also how many agree with this rip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtdbTrack {
    Matched {
        confidence: u32,
        total: u32,
    },
    Differs {
        confidence: u32,
        total: u32,
        samples: u32,
    },
    NoMatch {
        total: u32,
    },
}

/// The disc-level AccurateRip result: present when the log shows the lookup
/// happened at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccurateRipSummary {
    /// AccurateRip's three-part disc ID, when the log prints it.
    pub disc_ids: Option<(u32, u32, u32)>,
    pub in_database: bool,
}

/// The disc-level CUETools database result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtdbSummary {
    pub tocid: String,
    pub in_database: bool,
}

/// Why a text is not a rip log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RipLogError {
    #[error("text carries neither a ripper banner nor any track result")]
    NotARipLog,
}

/// Where a release's verification came from. Reading the log the rip left
/// beside the audio is one source; asking the databases directly is another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationSource {
    Log,
}

/// How many other copies of each track agree with this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    pub source: VerificationSource,
    pub tracks: Vec<TrackVerification>,
}

/// One track's agreement count from each database, and the CRC of the audio
/// those counts are about — the copy CRC, which is the checksum of the bits
/// that were kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackVerification {
    pub number: u32,
    /// Present only when the track matched: a mismatch's confidence belongs to
    /// the copy the database held, not to this rip.
    pub accuraterip_confidence: Option<u32>,
    pub ctdb_confidence: Option<u32>,
    pub crc: Option<u32>,
}

impl Verification {
    /// What a rip log claims about the release it describes.
    pub fn of(log: &RipLog) -> Self {
        Verification {
            source: VerificationSource::Log,
            tracks: log
                .tracks
                .iter()
                .map(|track| TrackVerification {
                    number: track.number,
                    accuraterip_confidence: match track.accuraterip {
                        AccurateRipTrack::Matched { confidence, .. } => Some(confidence),
                        _ => None,
                    },
                    ctdb_confidence: match track.ctdb {
                        Some(CtdbTrack::Matched { confidence, .. }) => Some(confidence),
                        _ => None,
                    },
                    crc: track.copy_crc,
                })
                .collect(),
        }
    }

    /// How many other rips of this release match, as one number: the weakest
    /// track's best database. A release is only as verified as the track
    /// fewest people confirmed, and a track no database confirmed leaves the
    /// release unverified altogether.
    pub fn matched_copies(&self) -> Option<u32> {
        self.tracks
            .iter()
            .map(|track| track.accuraterip_confidence.max(track.ctdb_confidence))
            .min()
            .flatten()
    }
}

/// Read a decoded rip log.
pub fn parse_rip_log(text: &str) -> Result<RipLog, RipLogError> {
    let mut parse = Parse::new();
    for line in text.lines() {
        parse.line(line);
    }
    parse.finish()
}

/// Where in the log the scan is. Per-track results come either in one block
/// per track or in a table of all of them, and never both at once, so the
/// position is one value rather than a set of flags to keep exclusive.
enum Section {
    /// Outside any of them: the banner, the settings, the TOC, the summaries.
    Elsewhere,
    Track(u32),
    /// CUERipper's `Track   [  CRC   |   V2   ] Status` table.
    CueRipperAccurateRip,
    /// CUERipper's `Track Peak [ CRC32  ] [W/O NULL]` table.
    CueRipperCrc,
}

struct Parse {
    ripper: Option<Ripper>,
    tracks: BTreeMap<u32, TrackResult>,
    accuraterip: Option<AccurateRipSummary>,
    ctdb: Option<CtdbSummary>,
    section: Section,
    /// The AccurateRip signatures XLD printed in the track block the scan is
    /// in, cleared on entering each one: its verdict line names a scheme but
    /// not the checksum that matched, which it printed a line or two earlier.
    v1_signature: Option<u32>,
    v2_signature: Option<u32>,
    plain_signature: Option<u32>,
}

impl Parse {
    fn new() -> Self {
        Parse {
            ripper: None,
            tracks: BTreeMap::new(),
            accuraterip: None,
            ctdb: None,
            section: Section::Elsewhere,
            v1_signature: None,
            v2_signature: None,
            plain_signature: None,
        }
    }

    fn finish(self) -> Result<RipLog, RipLogError> {
        let ripper = self.ripper.unwrap_or(Ripper::Unknown);
        if ripper == Ripper::Unknown && self.tracks.is_empty() {
            return Err(RipLogError::NotARipLog);
        }
        Ok(RipLog {
            ripper,
            tracks: self.tracks.into_values().collect(),
            accuraterip: self.accuraterip,
            ctdb: self.ctdb,
        })
    }

    fn line(&mut self, line: &str) {
        let line = line.trim();
        if self.ripper.is_none() {
            if let Some(ripper) = banner(line) {
                self.ripper = Some(ripper);
                return;
            }
        }
        if self.track_heading(line)
            || self.accuraterip_section(line)
            || self.ctdb_section(line)
            || self.summary_row(line)
            || self.ctdb_row(line)
            || self.cueripper_row(line)
            || self.track_line(line)
        {
            return;
        }
        // Any line CUERipper's tables did not claim ends them. Their rows run
        // unbroken, and what follows the AccurateRip one re-lists the same
        // tracks as read at other offsets, at counts that would otherwise
        // overwrite the real ones. A track block, by contrast, is full of
        // lines this parser has no use for and ends only at the next section.
        if matches!(
            self.section,
            Section::CueRipperAccurateRip | Section::CueRipperCrc
        ) {
            self.section = Section::Elsewhere;
        }
    }

    /// `Track  1` (EAC) or `Track 01` (XLD) on a line of its own opens a block
    /// of that track's results. Every other `Track` line carries a verdict or
    /// a table heading after the number.
    fn track_heading(&mut self, line: &str) -> bool {
        let Some(rest) = line.strip_prefix("Track") else {
            return false;
        };
        let Ok(number) = rest.trim().parse::<u32>() else {
            return false;
        };
        self.section = Section::Track(number);
        self.v1_signature = None;
        self.v2_signature = None;
        self.plain_signature = None;
        self.track_mut(number);
        true
    }

    fn track_mut(&mut self, number: u32) -> &mut TrackResult {
        self.tracks.entry(number).or_insert(TrackResult {
            number,
            test_crc: None,
            copy_crc: None,
            accuraterip: AccurateRipTrack::NotChecked,
            ctdb: None,
        })
    }

    fn accuraterip_mut(&mut self) -> &mut AccurateRipSummary {
        self.accuraterip.get_or_insert(AccurateRipSummary {
            disc_ids: None,
            in_database: true,
        })
    }

    /// The disc-level AccurateRip lines: the section heading EAC, XLD and
    /// CUERipper each print, CUERipper's disc ID line, and the verdicts EAC
    /// and XLD close with. EAC 0.99 prints its verdict with no heading above
    /// it, so any one of these is enough to say the lookup happened.
    fn accuraterip_section(&mut self, line: &str) -> bool {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("accuraterip summary") {
            self.section = Section::Elsewhere;
            let disc_ids = between(line, '(', ')').and_then(|inside| {
                disc_ids(inside.strip_prefix("DiscID:").unwrap_or_default().trim())
            });
            let summary = self.accuraterip_mut();
            if disc_ids.is_some() {
                summary.disc_ids = disc_ids;
            }
            return true;
        }
        if let Some(rest) = line.strip_prefix("[AccurateRip ID:") {
            self.section = Section::Elsewhere;
            let (id, status) = match rest.split_once(']') {
                Some(split) => split,
                None => return true,
            };
            let ids = disc_ids(id.trim());
            let found = !status.contains("not present");
            let summary = self.accuraterip_mut();
            summary.disc_ids = ids;
            summary.in_database = found;
            return true;
        }
        if lower.starts_with("disc not found in accuraterip db")
            || lower.starts_with("none of the tracks are present in the accuraterip database")
        {
            self.accuraterip_mut().in_database = false;
            return true;
        }
        // The per-disc verdicts. They add nothing a track row does not already
        // say, but a log whose tracks were all skipped still shows by their
        // presence that the lookup ran.
        if lower.starts_with("all tracks accurately ripped")
            || lower.starts_with("no tracks could be verified as accurate")
            || (lower.starts_with(char::is_numeric)
                && (lower.contains("track(s) accurately ripped")
                    || lower.contains("track(s) could not be verified as accurate")
                    || lower.contains("track(s) not present in the accuraterip database")))
        {
            self.accuraterip_mut();
            return true;
        }
        false
    }

    /// `[CTDB TOCID: …] found` / `… disk not present in database`, which EAC's
    /// plugin and CUERipper both print above the per-track table.
    fn ctdb_section(&mut self, line: &str) -> bool {
        let Some(rest) = line.strip_prefix("[CTDB TOCID:") else {
            return false;
        };
        self.section = Section::Elsewhere;
        let Some((tocid, status)) = rest.split_once(']') else {
            return true;
        };
        self.ctdb = Some(CtdbSummary {
            tocid: tocid.trim().to_string(),
            in_database: !status.contains("not present"),
        });
        true
    }

    /// EAC closes a range rip with `Track  1  accurately ripped …`; XLD opens
    /// with `Track 01 : OK (…)`. Both restate per-track results outside any
    /// track block.
    fn summary_row(&mut self, line: &str) -> bool {
        let Some((number, rest)) = numbered_row(line, "Track") else {
            return false;
        };
        if let Some(verdict) = rest.strip_prefix(':').and_then(|v| xld_summary(v.trim())) {
            self.record_accuraterip(number, verdict);
            return true;
        }
        if let Some(verdict) = eac_verdict(rest) {
            self.record_accuraterip(number, verdict);
            return true;
        }
        false
    }

    /// `  1   | (16/16) Accurately ripped` under `Track | CTDB Status`.
    fn ctdb_row(&mut self, line: &str) -> bool {
        if self.ctdb.is_none() {
            return false;
        }
        let Some((number, status)) = line.split_once('|') else {
            return false;
        };
        let Ok(number) = number.trim().parse::<u32>() else {
            return false;
        };
        // The TOC table is `number | start | length | …`; a CTDB row has one
        // field after the number and it opens with the match counts.
        let status = status.trim();
        if status.contains('|') {
            return false;
        }
        let Some(result) = ctdb_status(status) else {
            return false;
        };
        self.track_mut(number).ctdb = Some(result);
        true
    }

    /// CUERipper's two per-track tables: `Track   [  CRC   |   V2   ] Status`
    /// for the AccurateRip counts, `Track Peak [ CRC32  ] [W/O NULL]` for the
    /// CRC of the audio it wrote.
    fn cueripper_row(&mut self, line: &str) -> bool {
        if line.starts_with("Track") && line.contains("] Status") {
            self.section = Section::CueRipperAccurateRip;
            return true;
        }
        if line.starts_with("Track Peak") && line.contains("[ CRC32") {
            self.section = Section::CueRipperCrc;
            return true;
        }
        match self.section {
            Section::CueRipperAccurateRip => {
                let Some((number, rest)) = numbered_row(line, "") else {
                    return false;
                };
                let Some(verdict) = cueripper_verdict(rest) else {
                    return false;
                };
                self.record_accuraterip(number, verdict);
                true
            }
            Section::CueRipperCrc => {
                let Some(crc) = between(line, '[', ']').and_then(hex) else {
                    return false;
                };
                // The table opens with a `--` row carrying the whole disc's
                // CRC, which belongs to no track.
                if let Some((number, _)) = numbered_row(line, "") {
                    self.track_mut(number).copy_crc = Some(crc);
                }
                true
            }
            Section::Elsewhere | Section::Track(_) => false,
        }
    }

    /// The lines inside one track's block.
    fn track_line(&mut self, line: &str) -> bool {
        let Section::Track(number) = self.section else {
            return false;
        };
        if let Some(crc) = line.strip_prefix("Test CRC ").and_then(hex) {
            self.track_mut(number).test_crc = Some(crc);
            return true;
        }
        if let Some(crc) = line.strip_prefix("Copy CRC ").and_then(hex) {
            self.track_mut(number).copy_crc = Some(crc);
            return true;
        }
        if let Some((label, value)) = line.split_once(':') {
            let value = || hex(value.trim());
            match label.trim() {
                "CRC32 hash (test run)" => {
                    self.track_mut(number).test_crc = value();
                    return true;
                }
                "CRC32 hash" => {
                    self.track_mut(number).copy_crc = value();
                    return true;
                }
                "AccurateRip signature" => {
                    self.plain_signature = value();
                    return true;
                }
                "AccurateRip v1 signature" => {
                    self.v1_signature = value();
                    return true;
                }
                "AccurateRip v2 signature" => {
                    self.v2_signature = value();
                    return true;
                }
                _ => {}
            }
        }
        if let Some(verdict) = line.strip_prefix("->") {
            if let Some(verdict) = self.xld_verdict(verdict.trim()) {
                self.record_accuraterip(number, verdict);
                return true;
            }
            return false;
        }
        if let Some(verdict) = eac_verdict(line) {
            self.record_accuraterip(number, verdict);
            return true;
        }
        false
    }

    /// XLD's per-track verdict, with the checksum it matched filled in from
    /// the signatures printed above it in the same block.
    fn xld_verdict(&self, line: &str) -> Option<AccurateRipTrack> {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("track not present in accuraterip database") {
            return Some(AccurateRipTrack::NotInDatabase);
        }
        if lower.starts_with("rip may not be accurate (total") {
            return Some(AccurateRipTrack::Mismatch {
                confidence: None,
                crc: None,
                database_crc: None,
            });
        }
        // `->Accurately ripped! (confidence 1)` before XLD carried both
        // schemes; `->Accurately ripped (v1+v2, confidence 2+7/9)` after.
        let rest = lower
            .strip_prefix("accurately ripped!")
            .or_else(|| lower.strip_prefix("accurately ripped"))?;
        let inside = between(rest, '(', ')')?;
        let (version, confidence) = xld_confidence(inside)?;
        let crc = match version {
            ArVersion::V1 => self.v1_signature,
            ArVersion::V2 | ArVersion::Both => self.v2_signature,
            ArVersion::Unknown => self.plain_signature,
        };
        Some(AccurateRipTrack::Matched {
            confidence,
            version,
            crc,
        })
    }

    /// The later statement of a track's AccurateRip result wins: XLD's summary
    /// precedes the blocks that carry the signatures, EAC's follows the blocks
    /// and repeats them.
    fn record_accuraterip(&mut self, number: u32, result: AccurateRipTrack) {
        self.track_mut(number).accuraterip = result;
    }
}

fn banner(line: &str) -> Option<Ripper> {
    if let Some(rest) = line.strip_prefix("Exact Audio Copy V") {
        return Some(Ripper::Eac {
            version: rest.split(" from ").next().unwrap_or(rest).trim().to_string(),
        });
    }
    if let Some(rest) = line.strip_prefix("X Lossless Decoder version ") {
        return Some(Ripper::Xld {
            version: rest.trim().to_string(),
        });
    }
    if let Some(rest) = line.strip_prefix("CUERipper v") {
        return Some(Ripper::CueRipper {
            version: rest
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string(),
        });
    }
    None
}

/// A row that opens with a track number, optionally behind a label. Returns
/// the number and whatever follows it.
fn numbered_row<'a>(line: &'a str, label: &str) -> Option<(u32, &'a str)> {
    let rest = line.strip_prefix(label)?.trim_start();
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return None;
    }
    let (number, rest) = rest.split_at(digits);
    Some((number.parse().ok()?, rest.trim_start()))
}

/// EAC's verdict, printed identically in a track's block and in the summary a
/// range rip closes with. The negatives are checked first: "not accurately
/// ripped" contains "accurately ripped".
fn eac_verdict(line: &str) -> Option<AccurateRipTrack> {
    let lower = line.to_ascii_lowercase();
    if lower.starts_with("track not present in accuraterip database")
        || lower.starts_with("not present in database")
    {
        return Some(AccurateRipTrack::NotInDatabase);
    }
    let mismatch = [
        "cannot be verified as accurate",
        "not accurately ripped",
        "not ripped accurately",
    ]
    .iter()
    .any(|verdict| lower.starts_with(verdict));
    if !(mismatch || lower.starts_with("accurately ripped")) {
        return None;
    }
    let confidence = between(&lower, '(', ')')?
        .strip_prefix("confidence")?
        .trim()
        .parse()
        .ok()?;
    let mut checksums = line.split('[').skip(1).filter_map(|rest| {
        let (checksum, _) = rest.split_once(']')?;
        hex(checksum)
    });
    let crc = checksums.next()?;
    if mismatch {
        return Some(AccurateRipTrack::Mismatch {
            confidence: Some(confidence),
            crc: Some(crc),
            database_crc: checksums.next(),
        });
    }
    Some(AccurateRipTrack::Matched {
        confidence,
        version: ar_version(&lower),
        crc: Some(crc),
    })
}

/// The `(AR v1)` / `(AR v2)` tag EAC 1.x appends. EAC 0.99 appends nothing.
fn ar_version(lower: &str) -> ArVersion {
    if lower.contains("(ar v2)") {
        ArVersion::V2
    } else if lower.contains("(ar v1)") {
        ArVersion::V1
    } else {
        ArVersion::Unknown
    }
}

/// XLD's summary row body: `OK (v1+v2, confidence 9/9)` or
/// `NG (total 9 submissions)`. The row carries no checksum.
fn xld_summary(body: &str) -> Option<AccurateRipTrack> {
    if body.starts_with("NG") {
        return Some(AccurateRipTrack::Mismatch {
            confidence: None,
            crc: None,
            database_crc: None,
        });
    }
    let inside = between(body.strip_prefix("OK")?, '(', ')')?;
    let (version, confidence) = xld_confidence(inside)?;
    Some(AccurateRipTrack::Matched {
        confidence,
        version,
        crc: None,
    })
}

/// The parenthesised part of an XLD match: an optional scheme, then
/// `confidence <matches>/<submissions>`, where the matches are summed per
/// scheme (`2+7/9` is two v1 copies and seven v2 copies out of nine). Anything
/// after the count — `, with different offset` — describes how it matched, not
/// how many.
fn xld_confidence(inside: &str) -> Option<(ArVersion, u32)> {
    let mut fields = inside.split(',').map(str::trim);
    let first = fields.next()?;
    let (version, confidence) = match first.strip_prefix("confidence") {
        Some(count) => (ArVersion::Unknown, count),
        None => (
            xld_version(first)?,
            fields.next()?.strip_prefix("confidence")?,
        ),
    };
    // Older XLD prints the count alone, newer over the submissions it is out of.
    let counted = confidence.trim();
    let matches = counted
        .split_once('/')
        .map_or(counted, |(matches, _submissions)| matches)
        .split('+')
        .map(|count| count.trim().parse::<u32>())
        .sum::<Result<u32, _>>()
        .ok()?;
    Some((version, matches))
}

fn xld_version(field: &str) -> Option<ArVersion> {
    match field {
        "v1" => Some(ArVersion::V1),
        "v2" => Some(ArVersion::V2),
        "v1+v2" => Some(ArVersion::Both),
        _ => None,
    }
}

/// A CUERipper AccurateRip row: `[1c4d0bd3|46837ce2] (06+03/15) Accurately
/// ripped`, the two checksums being this rip's v1 and v2 signatures and the
/// counts how many copies of each the database matched.
fn cueripper_verdict(rest: &str) -> Option<AccurateRipTrack> {
    let checksums = between(rest, '[', ']')?;
    let counts = between(rest, '(', ')')?;
    let (matches, _submissions) = counts.split_once('/')?;
    let mut matches = matches.split('+').map(|count| count.trim().parse::<u32>());
    let v1 = matches.next()?.ok()?;
    let v2 = matches.next().transpose().ok()?.unwrap_or(0);
    let mut checksums = checksums.split('|').map(str::trim);
    let v1_crc = hex(checksums.next()?);
    let v2_crc = checksums.next().and_then(hex);
    let (version, crc) = match (v1 > 0, v2 > 0) {
        (true, true) => (ArVersion::Both, v2_crc),
        (false, true) => (ArVersion::V2, v2_crc),
        (true, false) => (ArVersion::V1, v1_crc),
        (false, false) => {
            return Some(AccurateRipTrack::Mismatch {
                confidence: None,
                crc: v2_crc.or(v1_crc),
                database_crc: None,
            })
        }
    };
    Some(AccurateRipTrack::Matched {
        confidence: v1 + v2,
        version,
        crc,
    })
}

/// A CUETools row body: `(16/16) Accurately ripped`, `(3/16) Differs in 12
/// samples @01:23:45`, `(0/16) No match`.
fn ctdb_status(status: &str) -> Option<CtdbTrack> {
    let counts = between(status, '(', ')')?;
    let (confidence, total) = counts.split_once('/')?;
    let confidence = confidence.trim().parse().ok()?;
    let total = total.trim().parse().ok()?;
    let verdict = status.split_once(')')?.1.trim().to_ascii_lowercase();
    if verdict.starts_with("accurately ripped") {
        return Some(CtdbTrack::Matched { confidence, total });
    }
    if verdict.starts_with("no match") {
        return Some(CtdbTrack::NoMatch { total });
    }
    let samples = verdict
        .strip_prefix("differs in")?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(CtdbTrack::Differs {
        confidence,
        total,
        samples,
    })
}

/// AccurateRip's disc ID: three hex fields, `0007b198-001eb52b-2a0a6804`.
fn disc_ids(text: &str) -> Option<(u32, u32, u32)> {
    let mut fields = text.split('-').map(hex);
    let ids = (fields.next()??, fields.next()??, fields.next()??);
    fields.next().is_none().then_some(ids)
}

fn between(text: &str, open: char, close: char) -> Option<&str> {
    let (_, rest) = text.split_once(open)?;
    let (inside, _) = rest.split_once(close)?;
    Some(inside.trim())
}

fn hex(text: &str) -> Option<u32> {
    u32::from_str_radix(text.trim(), 16).ok()
}

#[cfg(test)]
#[path = "rip_log_tests.rs"]
mod tests;
