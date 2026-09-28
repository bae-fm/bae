//! The medium a folder was ripped from, against the media a row states.
//!
//! What the folder's own files say about where its audio came from (see
//! [`crate::signals::rip`]) is a fact about the object on the desk, and a row
//! whose stated media could not have produced that audio describes some other
//! object. Such a row is still an answer a lookup gave, so it is set aside
//! rather than dropped — see [`super::combine`].

use super::row_facts::Fact;
use crate::pressing::{CdAudio, DiscogsDetail, Medium, StatedMedia};
use crate::signals::AudioOrigin;

/// What the folder's own files say about its audio, as combine reads it: the
/// rip evidence, which speaks to its medium; and whether the audio is one
/// channel, how many tracks it holds, what they are titled and where their
/// recordings were registered, which only tell otherwise tied rows apart.
#[derive(Debug, Clone, Copy)]
pub struct FolderAudio<'a> {
    pub origin: &'a AudioOrigin,
    /// Every audio file carries one channel.
    pub mono: bool,
    pub track_count: u32,
    /// Each track's title, in order — see
    /// [`crate::signals::Signals::track_titles`].
    pub track_titles: &'a [String],
    /// Where most of the tracks' recordings were registered, as their ISRC
    /// tags say — see [`crate::isrc::registered_in`].
    pub registered_in: Option<crate::pressing::ReleaseArea>,
}

impl FolderAudio<'static> {
    /// Files that prove nothing about their medium, and hold no tracks a
    /// tracklist could count.
    pub const UNPROVEN: Self = Self {
        origin: &AudioOrigin {
            source: None,
            not_cd_rate: None,
        },
        mono: false,
        track_count: 0,
        track_titles: &[],
        registered_in: None,
    };
}

/// What a run knows about the medium the folder's audio was ripped from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RippedFrom {
    Cd,
    NotCd,
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
    NotCdAudio,
}

/// Whether one-channel audio agrees with a row stating `details`: the row
/// states mono. Only Discogs states channels, as format descriptions.
///
/// Agreement only, never a contradiction. A row stating stereo is not ruled
/// out by mono files — catalogs list a mono pressing as stereo, and a stereo
/// master is folded down to one channel — so it only loses to an otherwise
/// tied row that states mono. Audio of two channels is never read against a
/// row: a mono record is routinely ripped to two identical channels, so two
/// channels prove nothing.
pub(crate) fn agrees_with_mono<'a>(
    mono_audio: bool,
    details: impl IntoIterator<Item = &'a DiscogsDetail>,
) -> bool {
    mono_audio
        && details.into_iter().any(|detail| {
            matches!(
                detail,
                DiscogsDetail::Mono | DiscogsDetail::T2TrackMono | DiscogsDetail::T4TrackMono
            )
        })
}

/// Whether a row whose records state `media` was released as the download
/// the folder is: a row stating a digital medium agrees, one stating only
/// physical carriers disagrees, and one naming no carrier states nothing. A
/// folder that is no download is weighed by [`RippedFrom`] instead.
pub(crate) fn download<'a>(
    origin: &AudioOrigin,
    media: impl IntoIterator<Item = &'a StatedMedia>,
) -> Fact {
    if !origin.is_download() {
        return Fact::StatesNothing;
    }
    let named: Vec<Medium> = media
        .into_iter()
        .flat_map(StatedMedia::entries)
        .flatten()
        .collect();
    if named.contains(&Medium::Digital) {
        Fact::Agrees
    } else if named.is_empty() {
        Fact::StatesNothing
    } else {
        Fact::Disagrees
    }
}

impl RippedFrom {
    /// What the folder's files say, and whether its disc ID named a release.
    /// A disc ID hashes a CD's table of contents, and one a catalog knows is
    /// a disc that was pressed: a folder whose track layout matches one to
    /// the frame was copied from it. A download proves no medium it was cut
    /// from — whatever its rate, the release it is a copy of is a digital one,
    /// which [`download`] weighs — so it sets no row aside.
    pub(crate) fn of(origin: &AudioOrigin, disc_id_matched: bool) -> Self {
        if origin.is_download() {
            Self::Unknown
        } else if origin.is_cd_rip() {
            Self::Cd
        } else if origin.not_cd_rate.is_some() {
            // The disc ID is not computed from a sheet whose audio rules a
            // CD out, so it cannot have matched here.
            Self::NotCd
        } else if disc_id_matched {
            Self::Cd
        } else {
            Self::Unknown
        }
    }

    /// What the folder proves, as the conflict it makes with rows that all
    /// state a medium it rules out. `None` where it proves nothing.
    pub(crate) fn conflict(self) -> Option<MediumConflict> {
        match self {
            Self::Cd => Some(MediumConflict::CdRip),
            Self::NotCd => Some(MediumConflict::NotCdAudio),
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
            Self::NotCd => CdAudio::Only,
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
    use super::download as download_fact;
    use super::*;
    use crate::pressing::StatedFormat;
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

    fn cd_rip() -> AudioOrigin {
        AudioOrigin {
            source: Some(crate::signals::AudioSource::CdRip {
                proof: CdProof::RipLog,
                file: None,
            }),
            not_cd_rate: None,
        }
    }

    fn not_cd_rate() -> AudioOrigin {
        AudioOrigin {
            source: None,
            not_cd_rate: Some(96_000),
        }
    }

    #[test]
    fn a_rip_the_files_prove_or_a_matched_disc_id_is_a_cd() {
        assert_eq!(RippedFrom::of(&cd_rip(), false), RippedFrom::Cd);
        assert_eq!(
            RippedFrom::of(&AudioOrigin::default(), true),
            RippedFrom::Cd
        );
        assert_eq!(
            RippedFrom::of(&AudioOrigin::default(), false),
            RippedFrom::Unknown
        );
        assert_eq!(RippedFrom::of(&not_cd_rate(), false), RippedFrom::NotCd);
    }

    /// A download proves no carrier, at whatever rate.
    #[test]
    fn a_download_proves_no_carrier() {
        let download =
            crate::signals::AudioSource::Download(crate::signals::DownloadProof::DeliverySet);
        let at_cd_rate = AudioOrigin {
            source: Some(download.clone()),
            not_cd_rate: None,
        };
        let at_96k = AudioOrigin {
            source: Some(download),
            not_cd_rate: Some(96_000),
        };
        assert_eq!(RippedFrom::of(&at_cd_rate, false), RippedFrom::Unknown);
        assert_eq!(RippedFrom::of(&at_96k, false), RippedFrom::Unknown);
    }

    /// A download agrees with a row stating a digital medium and disagrees
    /// with one stating only carriers; a row naming none states nothing, and
    /// a folder that is no download weighs no row this way.
    #[test]
    fn a_download_agrees_with_a_digital_release() {
        let download = AudioOrigin {
            source: Some(crate::signals::AudioSource::Download(
                crate::signals::DownloadProof::DeliverySet,
            )),
            not_cd_rate: None,
        };
        let digital = per_medium(&[Some(Medium::Digital)]);
        let cd = per_medium(&[Some(Medium::Cd)]);
        assert_eq!(download_fact(&download, [&digital]), Fact::Agrees);
        assert_eq!(download_fact(&download, [&cd, &digital]), Fact::Agrees);
        assert_eq!(download_fact(&download, [&cd]), Fact::Disagrees);
        assert_eq!(
            download_fact(&download, [&formats(&[Some(Medium::Vinyl)])]),
            Fact::Disagrees
        );
        assert_eq!(
            download_fact(&download, [&StatedMedia::Undescribed]),
            Fact::StatesNothing
        );
        assert_eq!(
            download_fact(&download, [&per_medium(&[None])]),
            Fact::StatesNothing
        );
        assert_eq!(download_fact(&cd_rip(), [&digital]), Fact::StatesNothing);
        assert_eq!(
            download_fact(&AudioOrigin::default(), [&cd]),
            Fact::StatesNothing
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
        let not_cd = RippedFrom::NotCd;
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

    /// A row stating mono agrees with mono audio; one stating only stereo,
    /// or nothing, does not, and two channels agree with no row.
    #[test]
    fn mono_audio_agrees_with_a_row_stating_mono() {
        use DiscogsDetail::{Mono, Stereo, T2TrackMono};
        assert!(agrees_with_mono(true, &[Mono]));
        assert!(agrees_with_mono(true, &[T2TrackMono]));
        assert!(agrees_with_mono(true, &[Mono, Stereo]));
        assert!(!agrees_with_mono(true, &[Stereo]));
        assert!(!agrees_with_mono(true, &[]));
        assert!(!agrees_with_mono(false, &[Mono]));
    }
}
