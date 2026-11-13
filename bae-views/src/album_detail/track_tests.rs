use super::*;

fn sided(letter: &str, number: u32) -> TrackPosition {
    TrackPosition::Sided { side_letter: letter.to_string(), number }
}

fn track(id: &str, position: TrackPosition) -> TrackDetail {
    TrackDetail {
        id: id.to_string(),
        title: String::new(),
        duration_ms: 180_000,
        artist_line: bae_types::credits::CreditLine::default(),
        display_artist: None,
        position_text: position.text(),
        position,
    }
}

#[test]
fn group_by_side_splits_on_side_letter() {
    let tracks =
        vec![track("a1", sided("A", 1)), track("a2", sided("A", 2)), track("b1", sided("B", 1))];
    let groups = group_tracks_by_side(&tracks);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].side, TrackSide::Sided { side_letter: "A".to_string() });
    assert_eq!(groups[0].tracks.len(), 2);
    assert_eq!(groups[1].side, TrackSide::Sided { side_letter: "B".to_string() });
    assert_eq!(groups[1].tracks.len(), 1);
}

#[test]
fn group_by_side_single_flat_group() {
    let tracks = vec![
        track("1", TrackPosition::Flat { number: 1 }),
        track("2", TrackPosition::Flat { number: 2 }),
    ];
    let groups = group_tracks_by_side(&tracks);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].side, TrackSide::Flat);
    assert_eq!(groups[0].tracks.len(), 2);
}
