//! Pure formatters: `fn data → String` with no state, no I/O, no context.
//!
//! Examples: `compute_track_position(format, side, …) → TrackPosition`.
//!
//! Zero dependencies on the database, the filesystem, or any struct
//! definition outside this file. Tests are `#[cfg(test)]` at the bottom.
//!
//! Called by `LibraryManager` when resolving raw `Db*` aggregates into
//! display-ready types in `crate::album_detail`. Also called by any other
//! module that needs the same deterministic formatting (e.g.
//! `import/search.rs`).

/// Convert a 1-indexed side number to a letter (1=A, 2=B, ..., 26=Z).
fn side_letter(side: i32) -> char {
    (b'A' + (side - 1) as u8) as char
}

/// Determine the format kind from the release format string.
enum FormatKind {
    Vinyl,
    Cassette,
    Digital, // CD, digital, or anything else
}

fn detect_format(format: Option<&str>) -> FormatKind {
    match format {
        Some(f) if f.contains("Vinyl") => FormatKind::Vinyl,
        Some(f) if f.contains("Cassette") => FormatKind::Cassette,
        _ => FormatKind::Digital,
    }
}

/// Whether a release format is a digital-style medium (CD, digital, or
/// anything unknown). Returns `false` for side-based physical formats
/// like vinyl or cassette, where "disc number" isn't the right label.
pub fn is_digital_format(format: Option<&str>) -> bool {
    matches!(detect_format(format), FormatKind::Digital)
}

/// Compute the structured [`crate::album_detail::TrackPosition`] for a track given the release
/// format and side count. Picks the case (sided physical / multi-disc digital /
/// flat) and fills its domain fields; the UI composes the position string and
/// resolves the header word. `side` is 1-indexed.
pub fn compute_track_position(
    format: Option<&str>,
    side: i32,
    track_number: Option<i32>,
    has_multiple_sides: bool,
) -> crate::album_detail::TrackPosition {
    use crate::album_detail::TrackPosition;
    let num = track_number.unwrap_or(1);
    match detect_format(format) {
        FormatKind::Vinyl | FormatKind::Cassette => TrackPosition::Sided {
            side_letter: side_letter(side).to_string(),
            number: num,
        },
        FormatKind::Digital => {
            if has_multiple_sides {
                TrackPosition::Disc {
                    disc: side,
                    number: num,
                }
            } else {
                TrackPosition::Flat { number: num }
            }
        }
    }
}

/// The side a track belongs to — the grouping discriminant. Every track on a
/// side yields the same value, so consecutive-equal grouping lands one group
/// per side. Carries no per-track number (a header has none).
fn track_side(position: &crate::album_detail::TrackPosition) -> crate::album_detail::TrackSide {
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
                continue;
            }
        }
        groups.push(crate::album_detail::TrackGroup {
            side,
            tracks: vec![track.clone()],
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

    fn track(id: &str, position: TrackPosition) -> TrackDetail {
        TrackDetail {
            id: id.to_string(),
            title: String::new(),
            side: 1,
            track_number: None,
            duration_ms: None,
            artist_names: String::new(),
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
