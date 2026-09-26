//! The medium a folder was ripped from, against the media a row states.
//!
//! What the folder's own files say about where its audio came from (see
//! [`crate::signals::rip`]) is a fact about the object on the desk, and a row
//! whose stated media could not have produced that audio describes some other
//! object. Such a row is still an answer a lookup gave, so it is set aside
//! rather than dropped — see [`super::combine`].

use crate::pressing::{CdAudio, DiscogsDetail, StatedMedia};
use crate::signals::RipEvidence;

/// What the folder's own files say about its medium, as combine reads it:
/// the rip evidence, and whether the audio is one channel.
#[derive(Debug, Clone, Copy)]
pub struct FolderAudio<'a> {
    pub rip: &'a RipEvidence,
    /// Every audio file carries one channel.
    pub mono: bool,
}

impl FolderAudio<'static> {
    /// Files that prove nothing about their medium.
    pub const UNPROVEN: Self = Self {
        rip: &RipEvidence::Unproven,
        mono: false,
    };
}

/// What a run knows about the medium the folder's audio was ripped from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RippedFrom {
    Cd,
    NotCd { sample_rate_hz: u32 },
    Unknown,
}

/// The folder's own files rule out every row the run found: what they prove,
/// against rows that all state a medium it could not have come from.
///
/// The rows stay on the list for a person to pick — a catalog may state the
/// wrong medium, or the one right pressing may not be there — but nothing
/// picks one for them: a verdict carrying this is never imported unattended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MediumConflict {
    /// The folder is a CD rip, and no row could be a CD.
    CdRip,
    /// The folder's audio is sampled at a rate no CD plays at, and every row
    /// is a CD.
    NotCdAudio { sample_rate_hz: u32 },
    /// The folder's audio is one channel, and every row states more.
    MonoAudio,
}

/// How mono audio stands to what a row's records state about its channels.
/// Only Discogs states channels, as format descriptions; a record that
/// states none says nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ChannelFit {
    /// The row states more channels than one, and not one: a one-channel
    /// file cannot hold what it describes.
    Contradicts,
    /// Nothing to go on: the audio is not mono, or the row states nothing,
    /// or it states both.
    Silent,
    /// The row states mono, as the audio is.
    Agrees,
}

impl ChannelFit {
    /// How mono audio stands to a row stating `details`. Audio of two
    /// channels is never read against a row: a mono record is routinely
    /// ripped to two identical channels, so two channels prove nothing.
    pub(crate) fn of<'a>(
        mono_audio: bool,
        details: impl IntoIterator<Item = &'a DiscogsDetail>,
    ) -> Self {
        if !mono_audio {
            return Self::Silent;
        }
        let (mut mono, mut more) = (false, false);
        for detail in details {
            match detail {
                DiscogsDetail::Mono | DiscogsDetail::T2TrackMono | DiscogsDetail::T4TrackMono => {
                    mono = true
                }
                DiscogsDetail::Stereo
                | DiscogsDetail::T2TrackStereo
                | DiscogsDetail::T4TrackStereo
                | DiscogsDetail::Quadraphonic
                | DiscogsDetail::Multichannel => more = true,
                _ => {}
            }
        }
        match (mono, more) {
            (true, _) => Self::Agrees,
            (false, true) => Self::Contradicts,
            (false, false) => Self::Silent,
        }
    }
}

impl RippedFrom {
    /// What the folder's files say, and whether its disc ID named a release.
    /// A disc ID hashes a CD's table of contents, and one a catalog knows is
    /// a disc that was pressed: a folder whose track layout matches one to
    /// the frame was copied from it.
    pub(crate) fn of(rip: &RipEvidence, disc_id_matched: bool) -> Self {
        match rip {
            RipEvidence::Cd { .. } => Self::Cd,
            // The disc ID is not computed from a sheet whose audio rules a
            // CD out, so it cannot have matched here.
            RipEvidence::NotCd { sample_rate_hz } => Self::NotCd {
                sample_rate_hz: *sample_rate_hz,
            },
            RipEvidence::Unproven if disc_id_matched => Self::Cd,
            RipEvidence::Unproven => Self::Unknown,
        }
    }

    /// What the folder proves, as the conflict it makes with rows that all
    /// state a medium it rules out. `None` where it proves nothing.
    pub(crate) fn conflict(self) -> Option<MediumConflict> {
        match self {
            Self::Cd => Some(MediumConflict::CdRip),
            Self::NotCd { sample_rate_hz } => Some(MediumConflict::NotCdAudio { sample_rate_hz }),
            Self::Unknown => None,
        }
    }

    /// Whether a row whose records state `media` could be what the folder was
    /// ripped from.
    ///
    /// Only a full account rules a row out: every medium its records list
    /// names a carrier, and none of those carriers could have given the
    /// folder its audio. A record that describes no media adds nothing to the
    /// account, and one medium whose carrier is unknown leaves it open — so a
    /// row stating nothing is never ruled out.
    pub(crate) fn admits<'a>(self, media: impl IntoIterator<Item = &'a StatedMedia>) -> bool {
        let ruled_out = match self {
            Self::Unknown => return true,
            // A CD rip cannot come off a carrier that plays no CD audio.
            Self::Cd => CdAudio::Never,
            // Audio that is not a CD's cannot come off a carrier that plays
            // nothing else.
            Self::NotCd { .. } => CdAudio::Only,
        };
        let entries: Vec<_> = media.into_iter().flat_map(StatedMedia::entries).collect();
        entries.is_empty()
            || !entries
                .iter()
                .all(|entry| entry.is_some_and(|medium| medium.cd_audio() == ruled_out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pressing::{Medium, StatedFormat};
    use crate::signals::CdProof;

    fn per_medium(media: &[Option<Medium>]) -> StatedMedia {
        StatedMedia::PerMedium(media.to_vec())
    }

    fn formats(media: &[Option<Medium>]) -> StatedMedia {
        StatedMedia::Formats(
            media
                .iter()
                .map(|medium| StatedFormat {
                    medium: *medium,
                    quantity: 1,
                })
                .collect(),
        )
    }

    const CD_RIP: RipEvidence = RipEvidence::Cd {
        proof: CdProof::RipLog,
        file: None,
    };

    #[test]
    fn a_rip_the_files_prove_or_a_matched_disc_id_is_a_cd() {
        assert_eq!(RippedFrom::of(&CD_RIP, false), RippedFrom::Cd);
        assert_eq!(RippedFrom::of(&RipEvidence::Unproven, true), RippedFrom::Cd);
        assert_eq!(
            RippedFrom::of(&RipEvidence::Unproven, false),
            RippedFrom::Unknown
        );
        assert_eq!(
            RippedFrom::of(
                &RipEvidence::NotCd {
                    sample_rate_hz: 96_000
                },
                false
            ),
            RippedFrom::NotCd {
                sample_rate_hz: 96_000
            }
        );
    }

    /// A CD rip rules out a row made only of carriers that play no CD audio,
    /// and nothing that could hold one.
    #[test]
    fn a_cd_rip_rules_out_only_rows_with_no_cd_among_them() {
        let cd = RippedFrom::Cd;
        assert!(!cd.admits([&per_medium(&[Some(Medium::Vinyl)])]));
        assert!(!cd.admits([&formats(&[Some(Medium::Vinyl), Some(Medium::Cassette)])]));
        assert!(cd.admits([&per_medium(&[Some(Medium::Cd)])]));
        assert!(cd.admits([&per_medium(&[Some(Medium::Cd), Some(Medium::Dvd)])]));
        assert!(cd.admits([&per_medium(&[Some(Medium::Sacd)])]));
        assert!(cd.admits([&per_medium(&[Some(Medium::DualDisc)])]));
        assert!(cd.admits([&StatedMedia::Undescribed]));
        assert!(cd.admits([&per_medium(&[Some(Medium::Vinyl), None])]));
    }

    /// Audio no CD holds rules out a row made only of CDs, and nothing with
    /// another carrier beside them.
    #[test]
    fn audio_off_a_cds_rate_rules_out_only_rows_of_cds() {
        let not_cd = RippedFrom::NotCd {
            sample_rate_hz: 96_000,
        };
        assert!(!not_cd.admits([&per_medium(&[Some(Medium::Cd), Some(Medium::Cd)])]));
        assert!(not_cd.admits([&per_medium(&[Some(Medium::Vinyl)])]));
        assert!(not_cd.admits([&per_medium(&[Some(Medium::Cd), Some(Medium::Dvd)])]));
        assert!(not_cd.admits([&per_medium(&[Some(Medium::Sacd)])]));
        assert!(not_cd.admits([&StatedMedia::Undescribed]));
    }

    /// A row's records are one account: a record that describes nothing adds
    /// nothing, and one that leaves a medium unnamed leaves the row open.
    #[test]
    fn a_rows_records_are_read_as_one_account() {
        let cd = RippedFrom::Cd;
        assert!(!cd.admits([&StatedMedia::Undescribed, &formats(&[Some(Medium::Vinyl)])]));
        assert!(cd.admits([&per_medium(&[Some(Medium::Vinyl)]), &formats(&[None])]));
        assert!(RippedFrom::Unknown.admits([&per_medium(&[Some(Medium::Vinyl)])]));
    }

    /// A row stating mono agrees with mono audio, one stating only stereo
    /// contradicts it, and two channels say nothing either way.
    #[test]
    fn mono_audio_reads_a_rows_channels() {
        use DiscogsDetail::{Mono, Stereo};
        assert_eq!(ChannelFit::of(true, &[Mono]), ChannelFit::Agrees);
        assert_eq!(ChannelFit::of(true, &[Stereo]), ChannelFit::Contradicts);
        assert_eq!(ChannelFit::of(true, &[Mono, Stereo]), ChannelFit::Agrees);
        assert_eq!(ChannelFit::of(true, &[]), ChannelFit::Silent);
        assert_eq!(ChannelFit::of(false, &[Mono]), ChannelFit::Silent);
        assert_eq!(ChannelFit::of(false, &[Stereo]), ChannelFit::Silent);
    }
}
