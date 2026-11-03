use super::*;
use bae_types::mediums::ReleaseMedium;
use bae_types::pressing::Medium;
use std::num::NonZeroU16;

/// A file or a Subsonic song states its track's medium as its disc, for a
/// release of several mediums only.
#[test]
fn the_disc_a_track_states_is_its_medium_of_several() {
    let row = |position: u16| {
        DbReleaseMedium::new(
            "release-id",
            NonZeroU16::new(position).expect("mediums count from one"),
            ReleaseMedium::one_sided(Some(Medium::Cd)),
            chrono::Utc::now(),
        )
    };
    let mut track = DbTrack::new_test("release-id", "track-id", "Track Title", 3);
    let one = DbReleaseMediums(vec![row(1)]);
    assert_eq!(one.disc_number(&track), None);
    let two = DbReleaseMediums(vec![row(1), row(2)]);
    track.medium_id.clone_from(&two.0[1].id);
    assert_eq!(two.disc_number(&track), Some(2));
}
