use super::*;

fn numbers(disc: Option<u32>, track: Option<u32>) -> TrackNumbers {
    TrackNumbers { disc, track }
}

#[test]
fn a_name_states_the_track_it_starts_with_and_a_disc_before_a_dash() {
    for (name, expected) in [
        ("07 Track Title.flac", numbers(None, Some(7))),
        ("07 - Track Title.flac", numbers(None, Some(7))),
        ("07-Track Title.flac", numbers(None, Some(7))),
        ("07. Track Title.flac", numbers(None, Some(7))),
        ("2-07 Track Title.flac", numbers(Some(2), Some(7))),
        ("CD2/2-07 Track Title.flac", numbers(Some(2), Some(7))),
        ("Track Title.flac", numbers(None, None)),
        ("1999 Track Title.flac", numbers(None, None)),
        ("00 Track Title.flac", numbers(None, None)),
    ] {
        assert_eq!(TrackNumbers::in_file_name(name), expected, "{name}");
    }
}

#[test]
fn a_name_fills_only_the_numbers_the_tags_leave_unstated() {
    assert_eq!(
        numbers(None, Some(3)).or_file_name("2-07 Track Title.flac"),
        numbers(Some(2), Some(3))
    );
    assert_eq!(
        numbers(None, None).or_file_name("Track Title.flac"),
        numbers(None, None)
    );
}

#[test]
fn files_play_by_disc_then_track_with_unstated_numbers_last() {
    let mut files = [
        numbers(None, None),
        numbers(Some(2), Some(1)),
        numbers(None, Some(1)),
        numbers(Some(1), Some(2)),
        numbers(Some(1), None),
        numbers(Some(1), Some(1)),
    ];
    files.sort_by(TrackNumbers::play_order);
    assert_eq!(
        files,
        [
            numbers(Some(1), Some(1)),
            numbers(Some(1), Some(2)),
            numbers(Some(1), None),
            numbers(Some(2), Some(1)),
            numbers(None, Some(1)),
            numbers(None, None),
        ]
    );
}
