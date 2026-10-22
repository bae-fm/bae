use super::*;

fn medium(carrier: Option<Medium>, sides: u8) -> ReleaseMedium {
    ReleaseMedium {
        carrier,
        sides: NonZeroU8::new(sides).expect("a medium has a side"),
    }
}

fn place(medium: u16, side: u8, number: u32) -> TrackPlace {
    TrackPlace {
        medium: NonZeroU16::new(medium).expect("mediums count from one"),
        side: NonZeroU8::new(side).expect("sides count from one"),
        number: NonZeroU32::new(number).expect("numbers count from one"),
    }
}

fn label(place: &TrackPlace, mediums: &[ReleaseMedium]) -> String {
    crate::util::format::track_position_text(&track_position(place, mediums))
}

/// Two records read A1 … D5: the second record's sides go on from the first's.
#[test]
fn a_two_record_set_letters_its_four_sides_in_order() {
    let mediums = [
        medium(Some(Medium::Vinyl), 2),
        medium(Some(Medium::Vinyl), 2),
    ];
    assert_eq!(label(&place(1, 1, 1), &mediums), "A1");
    assert_eq!(label(&place(1, 2, 6), &mediums), "B6");
    assert_eq!(label(&place(2, 1, 1), &mediums), "C1");
    assert_eq!(label(&place(2, 2, 5), &mediums), "D5");
}

/// The CDs of a box are discs, numbered among the mediums the release holds.
#[test]
fn the_discs_of_a_multi_disc_release_read_by_disc() {
    let mediums = [medium(Some(Medium::Cd), 1), medium(Some(Medium::Cd), 1)];
    assert_eq!(
        track_position(&place(2, 1, 3), &mediums),
        TrackPosition::Disc { disc: 2, number: 3 }
    );
    assert_eq!(
        track_side(&place(2, 1, 3), &mediums),
        TrackSide::Disc { disc: 2 }
    );
}

/// One medium with no sides to letter reads its numbers alone, whatever its
/// carrier.
#[test]
fn a_single_disc_reads_its_numbers_alone() {
    for carrier in [Some(Medium::Cd), None] {
        assert_eq!(
            track_position(&place(1, 1, 5), &[medium(carrier, 1)]),
            TrackPosition::Flat { number: 5 }
        );
    }
}

/// Past Z the letters go on as AA, AB, ….
#[test]
fn the_twenty_seventh_side_is_aa() {
    let mediums: Vec<_> = (0..14).map(|_| medium(Some(Medium::Vinyl), 2)).collect();
    assert_eq!(label(&place(13, 2, 1), &mediums), "Z1");
    assert_eq!(label(&place(14, 1, 1), &mediums), "AA1");
    assert_eq!(side_letters(28), "AB");
    assert_eq!(side_letters(52), "AZ");
    assert_eq!(side_letters(53), "BA");
}

/// A track whose source states no number is numbered by its place on its own
/// side, not in the whole release.
#[test]
fn an_unnumbered_track_takes_its_place_on_its_own_side() {
    let mut placer = Placer::default();
    let disc_one = medium_position(0);
    let disc_two = medium_position(1);
    let side = side_number(0);
    for number in 1..=3 {
        placer.place(disc_one, side, NonZeroU32::new(number));
    }
    placer.place(disc_two, side, NonZeroU32::new(1));
    assert_eq!(
        placer.place(disc_two, side, None),
        place(2, 1, 2),
        "the second track of disc 2"
    );
}

/// A file or a Subsonic song states its track's medium as its disc, for a
/// release of several mediums only.
#[test]
fn the_disc_a_track_states_is_its_medium_of_several() {
    let row = |position: u16| {
        crate::db::DbReleaseMedium::new(
            "release-id",
            NonZeroU16::new(position).expect("mediums count from one"),
            ReleaseMedium::one_sided(Some(Medium::Cd)),
            chrono::Utc::now(),
        )
    };
    let mut track = crate::db::DbTrack::new_test("release-id", "track-id", "Track Title", 3);
    let one = crate::db::DbReleaseMediums(vec![row(1)]);
    assert_eq!(one.disc_number(&track), None);
    let two = crate::db::DbReleaseMediums(vec![row(1), row(2)]);
    track.medium_id.clone_from(&two.0[1].id);
    assert_eq!(two.disc_number(&track), Some(2));
}
