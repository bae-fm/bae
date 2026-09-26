//! The medium a folder was ripped from, against the media a row states.
//!
//! What the folder's own files say about where its audio came from (see
//! [`crate::signals::rip`]) is a fact about the object on the desk, and a row
//! whose stated media could not have produced that audio describes some other
//! object. Such a row is still an answer a lookup gave, so it is set aside
//! rather than dropped — see [`super::combine`].

use crate::pressing::{CdAudio, StatedMedia};
use crate::signals::RipEvidence;

/// What a run knows about the medium the folder's audio was ripped from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RippedFrom {
    Cd,
    NotCd,
    Unknown,
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
            RipEvidence::NotCd { .. } => Self::NotCd,
            RipEvidence::Unproven if disc_id_matched => Self::Cd,
            RipEvidence::Unproven => Self::Unknown,
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
            RippedFrom::NotCd
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
}
