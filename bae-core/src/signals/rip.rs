//! The rip signal: what a candidate's own files say about the medium its audio
//! was ripped from.
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

/// What the candidate's files say about the medium its audio was ripped from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RipEvidence {
    /// A file proves a CD rip.
    Cd {
        proof: CdProof,
        /// The candidate-relative path of the file that proves it. `None` for
        /// a re-identify pass over a library release, whose files are its
        /// own rather than files of a scanned folder.
        file: Option<String>,
    },
    /// Every audio file is lossless and sampled at a rate other than a CD's
    /// 44.1 kHz, so none of it can have come off a CD as it is.
    NotCd { sample_rate_hz: u32 },
    /// Nothing either way.
    Unproven,
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
