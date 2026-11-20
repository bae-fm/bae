use super::*;

fn place(medium: u16, side: u8, number: u32) -> TrackPlace {
    TrackPlace {
        medium: NonZeroU16::new(medium).expect("mediums count from one"),
        side: NonZeroU8::new(side).expect("sides count from one"),
        number: NonZeroU32::new(number).expect("numbers count from one"),
    }
}

/// Past Z the letters go on as AA, AB, ….
#[test]
fn the_twenty_seventh_side_is_aa() {
    assert_eq!(side_letters(26), "Z");
    assert_eq!(side_letters(27), "AA");
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
    assert_eq!(placer.place(disc_two, side, None), place(2, 1, 2), "the second track of disc 2");
}

#[test]
fn track_position_text_renders_each_variant() {
    assert_eq!(TrackPosition::Sided { side_letter: "A".to_string(), number: 2 }.text(), "A2");
    assert_eq!(TrackPosition::Disc { disc: 2, number: 3 }.text(), "3");
    assert_eq!(TrackPosition::Flat { number: 5 }.text(), "5");
}
