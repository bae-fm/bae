//! Where a candidate's audio came from, as its own files say: ripped off a CD,
//! bought as a download, or sampled at a rate no CD plays at.
//!
//! A folder states its medium the way nothing else in it can: a CD ripper
//! leaves files behind that only a disc it read produces, and audio sampled at
//! a rate a CD does not play at cannot have come off one. Either settles which
//! of the rows a run finds describe the object the folder was copied from, and
//! whether the folder's track sheet can be hashed into a disc ID worth asking
//! about.
//!
//! What is not proof: a track sheet on its own, which is written for a vinyl
//! rip as readily as for a CD; and a folder at a CD's rate, which a
//! transfer from any source can be. Those say nothing, and a folder that says
//! nothing has no row set aside for its medium.
//!
//! A download is proven by what a store writes into the files it sells, or by
//! the set of tags a label delivers with every track — an ISRC, a phonographic
//! copyright line and the label — where no rip log, AccurateRip report or
//! track sheet says a disc was read. The encoder, genre and date prove
//! nothing: rips carry them too.

/// A file only a CD rip leaves behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdProof {
    /// A rip log whose table of contents reads: the layout of a disc a CD
    /// ripper read.
    RipLog,
    /// An AccurateRip report that found the disc in AccurateRip's database,
    /// which holds CDs alone.
    AccurateRipReport,
    /// A track sheet a CD ripper wrote (see [`crate::cue_flac::CdRipper`]).
    RipperSheet,
}

impl CdProof {
    /// The word a stored signal keeps it as.
    pub fn key(self) -> &'static str {
        match self {
            Self::RipLog => "rip_log",
            Self::AccurateRipReport => "accurate_rip_report",
            Self::RipperSheet => "ripper_sheet",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        [Self::RipLog, Self::AccurateRipReport, Self::RipperSheet]
            .into_iter()
            .find(|proof| proof.key() == key)
    }
}

/// A store the tags say a file was bought from — only what a store itself
/// writes into the files it delivers, never what a tagger may.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreMarker {
    /// An iTunes Store purchase: the MP4 atoms iTunes writes only into a
    /// purchased file — the purchase date (`purd`), the buyer's account
    /// (`apID`) or name (`ownr`). The catalog ids beside them (`cnID`,
    /// `plID`, `atID`, `sfID`) are also on iTunes Match and Apple Music
    /// copies, so they do not say it was bought.
    ITunesPurchase,
    /// A Bandcamp download: its comment reads
    /// `Visit https://<artist>.bandcamp.com`.
    Bandcamp,
}

impl StoreMarker {
    /// The word a stored reading keeps it as.
    pub fn key(self) -> &'static str {
        match self {
            Self::ITunesPurchase => "itunes_purchase",
            Self::Bandcamp => "bandcamp",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        [Self::ITunesPurchase, Self::Bandcamp]
            .into_iter()
            .find(|marker| marker.key() == key)
    }
}

/// What proves the audio was bought as a download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadProof {
    /// A store's own marker, on the file at this candidate-relative path.
    Store { marker: StoreMarker, file: String },
    /// Every track carries what a label delivers with a download: an ISRC,
    /// a phonographic copyright line, and the label.
    DeliverySet,
}

/// Where the audio came from, as a file proves it. A CD rip and a download
/// exclude each other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioSource {
    /// A file only a CD rip leaves behind.
    CdRip {
        proof: CdProof,
        /// The candidate-relative path of the file that proves it. `None` for
        /// a re-identify pass over a library release, whose files are its
        /// own rather than files of a scanned folder.
        file: Option<String>,
    },
    Download(DownloadProof),
}

/// What the candidate's files say about where its audio came from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioOrigin {
    /// What a file proves the audio was taken from, when one does.
    pub source: Option<AudioSource>,
    /// The rate the audio is sampled at, when every file is lossless and none
    /// is at a CD's 44.1 kHz — so none of it came off a CD as it is.
    pub not_cd_rate: Option<u32>,
}

impl AudioOrigin {
    /// Whether a file proves a CD rip.
    pub fn is_cd_rip(&self) -> bool {
        matches!(self.source, Some(AudioSource::CdRip { .. }))
    }

    /// Whether a file proves a download.
    pub fn is_download(&self) -> bool {
        matches!(self.source, Some(AudioSource::Download(_)))
    }
}

/// Whether a copyright line states a phonographic copyright: "℗" or "(P)".
pub(crate) fn states_phonographic_copyright(line: &str) -> bool {
    line.contains('℗') || line.to_ascii_uppercase().contains("(P)")
}

/// A CD plays 44,100 samples a second.
pub const CD_SAMPLE_RATE_HZ: u32 = 44_100;

/// The rate that rules a CD out, when the audio rules it out: every file is
/// lossless — a lossy encoder may resample a CD's audio on its own, so a lossy
/// file's rate says nothing about its source — and none is at a CD's rate.
/// The first file's rate stands for them.
///
/// Neither the channels nor the bit depth are read. A mono CD is often kept as
/// a one-channel file, and an HDCD decodes to more than 16 bits, so a CD rip
/// can carry either and stay one.
pub fn rate_ruling_out_cd<'a>(
    audio: impl IntoIterator<Item = &'a crate::album_detail::AudioFormat>,
) -> Option<u32> {
    let mut first = None;
    for format in audio {
        let rate = u32::try_from(format.sample_rate_hz).ok()?;
        if format.bits_per_sample.is_none() || rate == CD_SAMPLE_RATE_HZ {
            return None;
        }
        first.get_or_insert(rate);
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::album_detail::AudioFormat;

    fn format(sample_rate_hz: i64, bits_per_sample: Option<i64>, channels: i64) -> AudioFormat {
        AudioFormat {
            codec: "FLAC".to_string(),
            sample_rate_hz,
            bits_per_sample,
            bitrate_kbps: bits_per_sample.is_none().then_some(320),
            channels,
        }
    }

    /// Lossless audio at a rate a CD does not play at rules a CD out, mono or
    /// not.
    #[test]
    fn lossless_audio_off_a_cds_rate_rules_it_out() {
        assert_eq!(
            rate_ruling_out_cd(&[format(96_000, Some(24), 1)]),
            Some(96_000)
        );
        assert_eq!(
            rate_ruling_out_cd(&[format(48_000, Some(24), 2), format(96_000, Some(24), 2)]),
            Some(48_000)
        );
    }

    /// A CD's rate — whatever the channels or bit depth — lossy audio, one
    /// file at a CD's rate among others, and no audio at all say nothing.
    #[test]
    fn audio_that_could_be_a_cds_says_nothing() {
        assert_eq!(rate_ruling_out_cd(&[format(44_100, Some(16), 2)]), None);
        assert_eq!(rate_ruling_out_cd(&[format(44_100, Some(24), 1)]), None);
        assert_eq!(rate_ruling_out_cd(&[format(48_000, None, 2)]), None);
        assert_eq!(
            rate_ruling_out_cd(&[format(96_000, Some(24), 2), format(44_100, Some(16), 2)]),
            None
        );
        assert_eq!(rate_ruling_out_cd(&[]), None);
    }

    #[test]
    fn a_proof_reads_back_from_its_key() {
        for proof in [
            CdProof::RipLog,
            CdProof::AccurateRipReport,
            CdProof::RipperSheet,
        ] {
            assert_eq!(CdProof::from_key(proof.key()), Some(proof));
        }
    }
}
