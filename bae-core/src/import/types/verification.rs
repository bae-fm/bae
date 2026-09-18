//! How many other copies of a release's audio agree with this one.
//!
//! A rip looks each track up in AccurateRip — and, where the ripper supports
//! it, in the CUETools database — and the answer is a count of other people's
//! copies that carry the same bits. That count is what a release is verified
//! by: the audio is not judged against a reference, it is judged against
//! everybody else's reading of the same disc.
//!
//! What a catalog says about the release is a record; what its own folder
//! states is a mark; what the databases confirmed about its bits is this.

/// Where a release's verification came from. Reading the log the rip left
/// beside the audio is one source; asking the databases directly is another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerificationSource {
    Log,
}

impl VerificationSource {
    /// Every source, in the order a surface would list them.
    pub const ALL: [VerificationSource; 1] = [Self::Log];

    /// The stored `source` column value.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Log => "log",
        }
    }
}

impl std::str::FromStr for VerificationSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|source| source.as_str() == s)
            .ok_or_else(|| format!("unknown verification source: {s}"))
    }
}

impl std::fmt::Display for VerificationSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
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
