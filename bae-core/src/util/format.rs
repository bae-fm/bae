//! Pure formatters over `crate::album_detail`'s position types: no state, no
//! I/O, no database. `compute_track_position` picks a track's position case from
//! the release format; `track_position_text` renders it ("A1"); the rest
//! classify the format and group tracks by side.

/// Convert a 1-indexed side number to a letter (1=A, 2=B, ..., 26=Z).
pub fn side_letter(side: i32) -> String {
    ((b'A' + (side - 1) as u8) as char).to_string()
}

/// A physical release format whose tracks are laid out on sides, not discs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalSideMedium {
    Vinyl,
    Cassette,
}

enum FormatKind {
    Sided(PhysicalSideMedium),
    Digital, // CD, digital, or anything else
}

fn detect_format(format: Option<&str>) -> FormatKind {
    match format {
        Some(f) if f.contains("Vinyl") => FormatKind::Sided(PhysicalSideMedium::Vinyl),
        Some(f) if f.contains("Cassette") => FormatKind::Sided(PhysicalSideMedium::Cassette),
        _ => FormatKind::Digital,
    }
}

/// The physical medium for a release format that has sides.
pub fn physical_side_medium(format: Option<&str>) -> Option<PhysicalSideMedium> {
    match detect_format(format) {
        FormatKind::Sided(medium) => Some(medium),
        FormatKind::Digital => None,
    }
}

/// Whether a release format is a digital-style medium (CD, digital, or
/// anything unknown). Returns `false` for side-based physical formats
/// like vinyl or cassette, where "disc number" isn't the right label.
pub fn is_digital_format(format: Option<&str>) -> bool {
    matches!(detect_format(format), FormatKind::Digital)
}

/// The structured [`crate::album_detail::TrackPosition`] for a track: picks the
/// case (sided physical / multi-disc digital / flat) from the release format and
/// fills its domain fields. `side` is 1-indexed.
pub fn compute_track_position(
    format: Option<&str>,
    side: i32,
    track_number: Option<i32>,
    has_multiple_sides: bool,
) -> crate::album_detail::TrackPosition {
    use crate::album_detail::TrackPosition;
    match detect_format(format) {
        FormatKind::Sided(_) => match track_number {
            Some(number) => TrackPosition::Sided {
                side_letter: side_letter(side),
                number,
            },
            None => TrackPosition::SidedUnnumbered {
                side_letter: side_letter(side),
            },
        },
        FormatKind::Digital => {
            if has_multiple_sides {
                match track_number {
                    Some(number) => TrackPosition::Disc { disc: side, number },
                    None => TrackPosition::DiscUnnumbered { disc: side },
                }
            } else {
                match track_number {
                    Some(number) => TrackPosition::Flat { number },
                    None => TrackPosition::Unnumbered,
                }
            }
        }
    }
}

/// Render a track's position for a table without side group headers — the
/// import mapping pane. The disc case carries its disc number ("2-3"), since
/// no header above the row states it. `None` where there is no number to
/// print.
pub fn ungrouped_track_position_text(
    position: &crate::album_detail::TrackPosition,
) -> Option<String> {
    use crate::album_detail::TrackPosition;
    match position {
        TrackPosition::Sided {
            side_letter,
            number,
        } => Some(format!("{side_letter}{number}")),
        TrackPosition::SidedUnnumbered { side_letter } => Some(side_letter.clone()),
        TrackPosition::Disc { disc, number } => Some(format!("{disc}-{number}")),
        TrackPosition::Flat { number } => Some(number.to_string()),
        TrackPosition::DiscUnnumbered { .. } | TrackPosition::Unnumbered => None,
    }
}

/// Render the per-track position label from the structured position.
pub fn track_position_text(position: &crate::album_detail::TrackPosition) -> String {
    use crate::album_detail::TrackPosition;
    match position {
        TrackPosition::Sided {
            side_letter,
            number,
        } => format!("{side_letter}{number}"),
        TrackPosition::SidedUnnumbered { side_letter } => side_letter.clone(),
        // The disc is the group header; repeating it on every row is noise.
        TrackPosition::Disc { number, .. } => number.to_string(),
        TrackPosition::DiscUnnumbered { .. } => "-".to_string(),
        TrackPosition::Flat { number } => number.to_string(),
        TrackPosition::Unnumbered => "-".to_string(),
    }
}

/// The side a track belongs to — the grouping discriminant. Every track on a
/// side yields the same value, so consecutive-equal grouping lands one group
/// per side. Carries no per-track number (a header has none).
fn track_side(position: &crate::album_detail::TrackPosition) -> crate::album_detail::TrackSide {
    use crate::album_detail::{TrackPosition, TrackSide};
    match position {
        TrackPosition::Sided { side_letter, .. }
        | TrackPosition::SidedUnnumbered { side_letter } => TrackSide::Sided {
            side_letter: side_letter.clone(),
        },
        TrackPosition::Disc { disc, .. } | TrackPosition::DiscUnnumbered { disc } => {
            TrackSide::Disc { disc: *disc }
        }
        TrackPosition::Flat { .. } | TrackPosition::Unnumbered => TrackSide::Flat,
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
        let duration_ms = track.duration_ms.unwrap_or(0);
        if let Some(last) = groups.last_mut() {
            if last.side == side {
                last.tracks.push(track.clone());
                last.total_duration_ms += duration_ms;
                continue;
            }
        }
        groups.push(crate::album_detail::TrackGroup {
            side,
            tracks: vec![track.clone()],
            total_duration_ms: duration_ms,
        });
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::album_detail::{TrackDetail, TrackPosition};

    fn sided(letter: &str, number: i32) -> TrackPosition {
        TrackPosition::Sided {
            side_letter: letter.to_string(),
            number,
        }
    }

    #[test]
    fn test_vinyl_position() {
        assert_eq!(
            compute_track_position(Some("2xLP, Vinyl"), 1, Some(2), true),
            sided("A", 2)
        );
        assert_eq!(
            compute_track_position(Some("Vinyl"), 3, Some(1), true),
            sided("C", 1)
        );
    }

    #[test]
    fn physical_side_medium_detection() {
        assert_eq!(
            physical_side_medium(Some("2xLP, Vinyl")),
            Some(PhysicalSideMedium::Vinyl)
        );
        assert_eq!(
            physical_side_medium(Some("Cassette")),
            Some(PhysicalSideMedium::Cassette)
        );
        assert_eq!(physical_side_medium(Some("2xCD")), None);
        assert_eq!(physical_side_medium(None), None);
    }

    #[test]
    fn test_cassette_position() {
        assert_eq!(
            compute_track_position(Some("Cassette"), 2, Some(3), true),
            sided("B", 3)
        );
    }

    #[test]
    fn test_cd_single_disc() {
        assert_eq!(
            compute_track_position(Some("CD"), 1, Some(5), false),
            TrackPosition::Flat { number: 5 }
        );
    }

    #[test]
    fn test_cd_multi_disc() {
        assert_eq!(
            compute_track_position(Some("2xCD"), 2, Some(3), true),
            TrackPosition::Disc { disc: 2, number: 3 }
        );
    }

    #[test]
    fn test_no_format() {
        assert_eq!(
            compute_track_position(None, 1, Some(1), false),
            TrackPosition::Flat { number: 1 }
        );
    }

    #[test]
    fn missing_track_number_is_explicit_position_state() {
        assert_eq!(
            compute_track_position(Some("Vinyl"), 1, None, true),
            TrackPosition::SidedUnnumbered {
                side_letter: "A".to_string()
            }
        );
        assert_eq!(
            compute_track_position(Some("2xCD"), 2, None, true),
            TrackPosition::DiscUnnumbered { disc: 2 }
        );
        assert_eq!(
            compute_track_position(Some("CD"), 1, None, false),
            TrackPosition::Unnumbered
        );
    }

    #[test]
    fn track_position_text_renders_numbered_and_unnumbered_variants() {
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
        assert_eq!(
            track_position_text(&TrackPosition::SidedUnnumbered {
                side_letter: "A".to_string()
            }),
            "A"
        );
        assert_eq!(
            track_position_text(&TrackPosition::DiscUnnumbered { disc: 2 }),
            "-"
        );
        assert_eq!(track_position_text(&TrackPosition::Unnumbered), "-");
    }

    fn track(id: &str, position: TrackPosition) -> TrackDetail {
        TrackDetail {
            id: id.to_string(),
            title: String::new(),
            side: 1,
            track_number: None,
            duration_ms: None,
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
