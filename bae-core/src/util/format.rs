//! Pure formatters over `crate::album_detail`'s position types: no state, no
//! I/O, no database. `track_position_text` renders a position ("A1"); the rest
//! group tracks by side. Where a track sits is `crate::mediums`'.

/// Render the per-track position label from the structured position.
pub fn track_position_text(position: &crate::album_detail::TrackPosition) -> String {
    use crate::album_detail::TrackPosition;
    match position {
        TrackPosition::Sided {
            side_letter,
            number,
        } => format!("{side_letter}{number}"),
        // The disc is the group header; repeating it on every row is noise.
        TrackPosition::Disc { number, .. } | TrackPosition::Flat { number } => number.to_string(),
    }
}

/// The side a track belongs to — the grouping discriminant. Every track on a
/// side yields the same value, so consecutive-equal grouping lands one group
/// per side. Carries no per-track number (a header has none).
pub fn track_side(position: &crate::album_detail::TrackPosition) -> crate::album_detail::TrackSide {
    use crate::album_detail::{TrackPosition, TrackSide};
    match position {
        TrackPosition::Sided { side_letter, .. } => TrackSide::Sided {
            side_letter: side_letter.clone(),
        },
        TrackPosition::Disc { disc, .. } => TrackSide::Disc { disc: *disc },
        TrackPosition::Flat { .. } => TrackSide::Flat,
    }
}

/// Group pre-sorted tracks by consecutive side. Two tracks share a group when
/// their positions resolve to the same [`crate::album_detail::TrackSide`] (same side letter, same
/// disc, or both flat).
pub fn group_tracks_by_side(
    tracks: &[crate::album_detail::TrackDetail],
) -> Vec<crate::album_detail::TrackGroup> {
    let mut groups: Vec<crate::album_detail::TrackGroup> = Vec::new();
    for track in tracks {
        let side = track_side(&track.position);
        if let Some(last) = groups.last_mut() {
            if last.side == side {
                last.tracks.push(track.clone());
                last.total_duration_ms += track.duration_ms;
                continue;
            }
        }
        groups.push(crate::album_detail::TrackGroup {
            side,
            tracks: vec![track.clone()],
            total_duration_ms: track.duration_ms,
        });
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::album_detail::{TrackDetail, TrackPosition};

    fn sided(letter: &str, number: u32) -> TrackPosition {
        TrackPosition::Sided {
            side_letter: letter.to_string(),
            number,
        }
    }

    #[test]
    fn track_position_text_renders_each_variant() {
        assert_eq!(
            track_position_text(&TrackPosition::Sided {
                side_letter: "A".to_string(),
                number: 2
            }),
            "A2"
        );
        assert_eq!(
            track_position_text(&TrackPosition::Disc { disc: 2, number: 3 }),
            "3"
        );
        assert_eq!(track_position_text(&TrackPosition::Flat { number: 5 }), "5");
    }

    fn track(id: &str, position: TrackPosition) -> TrackDetail {
        TrackDetail {
            id: id.to_string(),
            title: String::new(),
            duration_ms: 180_000,
            artist_names: String::new(),
            display_artist: None,
            position_text: track_position_text(&position),
            position,
        }
    }

    #[test]
    fn group_by_side_splits_on_side_letter() {
        use crate::album_detail::TrackSide;
        let tracks = vec![
            track("a1", sided("A", 1)),
            track("a2", sided("A", 2)),
            track("b1", sided("B", 1)),
        ];
        let groups = group_tracks_by_side(&tracks);
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups[0].side,
            TrackSide::Sided {
                side_letter: "A".to_string()
            }
        );
        assert_eq!(groups[0].tracks.len(), 2);
        assert_eq!(
            groups[1].side,
            TrackSide::Sided {
                side_letter: "B".to_string()
            }
        );
        assert_eq!(groups[1].tracks.len(), 1);
    }

    #[test]
    fn group_by_side_single_flat_group() {
        use crate::album_detail::TrackSide;
        let tracks = vec![
            track("1", TrackPosition::Flat { number: 1 }),
            track("2", TrackPosition::Flat { number: 2 }),
        ];
        let groups = group_tracks_by_side(&tracks);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].side, TrackSide::Flat);
        assert_eq!(groups[0].tracks.len(), 2);
    }
}
