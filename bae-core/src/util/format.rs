//! Pure formatters: `fn data → String` with no state, no I/O, no context.
//!
//! Examples: `format_minutes_seconds(ms) → "3:07"`,
//! `compute_track_labels(format, side, …) → (String, String)`.
//!
//! Zero dependencies on the database, the filesystem, or any struct
//! definition outside this file. Tests are `#[cfg(test)]` at the bottom.
//!
//! Called by `LibraryManager` when resolving raw `Db*` aggregates into
//! display-ready types in `crate::album_detail`. Also called by any other
//! module that needs the same deterministic formatting (e.g.
//! `import/search.rs`).

/// Format an unsigned millisecond duration as a bare "M:SS" clock label
/// (e.g. "3:07", "14:59"). Seconds floor to the whole second.
pub fn format_minutes_seconds(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{}:{:02}", minutes, seconds)
}

/// Format an ETA from progress, total size, and download rate.
/// Returns empty string when ETA can't be computed (rate is zero or download complete).
pub fn format_eta(progress: f64, total_bytes: u64, bytes_per_sec: u64) -> String {
    if bytes_per_sec == 0 || progress >= 1.0 {
        return String::new();
    }
    let remaining = (total_bytes as f64 * (1.0 - progress)) as u64;
    let seconds = remaining / bytes_per_sec;
    if seconds < 60 {
        return format!("{}s remaining", seconds);
    }
    let minutes = seconds / 60;
    let secs = seconds % 60;
    if minutes < 60 {
        return format!("{}m {}s remaining", minutes, secs);
    }
    let hours = minutes / 60;
    let mins = minutes % 60;
    format!("{}h {}m remaining", hours, mins)
}

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

/// Compute side_label and position_label for a track given the release format and side count.
pub fn compute_track_labels(
    format: Option<&str>,
    side: i32,
    track_number: Option<i32>,
    has_multiple_sides: bool,
) -> (String, String) {
    let num = track_number.unwrap_or(1);
    match detect_format(format) {
        FormatKind::Vinyl => {
            let letter = side_letter(side);
            let side_label = format!("Side {letter}");
            let position_label = format!("{letter}{num}");
            (side_label, position_label)
        }
        FormatKind::Cassette => {
            let letter = side_letter(side);
            let side_label = format!("Side {letter}");
            let position_label = format!("{letter}{num}");
            (side_label, position_label)
        }
        FormatKind::Digital => {
            let position_label = if has_multiple_sides {
                format!("{side}-{num}")
            } else {
                num.to_string()
            };
            let side_label = if has_multiple_sides {
                format!("Disc {side}")
            } else {
                String::new()
            };
            (side_label, position_label)
        }
    }
}

/// Group pre-sorted tracks by consecutive side_label.
pub fn group_tracks_by_side(
    tracks: &[crate::album_detail::TrackDetail],
) -> Vec<crate::album_detail::TrackGroup> {
    let mut groups: Vec<crate::album_detail::TrackGroup> = Vec::new();
    for track in tracks {
        if let Some(last) = groups.last_mut() {
            if last.side_label == track.side_label {
                last.tracks.push(track.clone());
                continue;
            }
        }
        groups.push(crate::album_detail::TrackGroup {
            side_label: track.side_label.clone(),
            tracks: vec![track.clone()],
        });
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minutes_seconds_basic() {
        assert_eq!(format_minutes_seconds(0), "0:00");
        assert_eq!(format_minutes_seconds(5_000), "0:05");
        assert_eq!(format_minutes_seconds(63_000), "1:03");
        assert_eq!(format_minutes_seconds(187_000), "3:07");
        assert_eq!(format_minutes_seconds(899_000), "14:59");
    }

    #[test]
    fn test_vinyl_labels() {
        let (side, pos) = compute_track_labels(Some("2xLP, Vinyl"), 1, Some(2), true);
        assert_eq!(side, "Side A");
        assert_eq!(pos, "A2");

        let (side, pos) = compute_track_labels(Some("Vinyl"), 3, Some(1), true);
        assert_eq!(side, "Side C");
        assert_eq!(pos, "C1");
    }

    #[test]
    fn test_cassette_labels() {
        let (side, pos) = compute_track_labels(Some("Cassette"), 2, Some(3), true);
        assert_eq!(side, "Side B");
        assert_eq!(pos, "B3");
    }

    #[test]
    fn test_cd_single_disc() {
        let (side, pos) = compute_track_labels(Some("CD"), 1, Some(5), false);
        assert_eq!(side, "");
        assert_eq!(pos, "5");
    }

    #[test]
    fn test_cd_multi_disc() {
        let (side, pos) = compute_track_labels(Some("2xCD"), 2, Some(3), true);
        assert_eq!(side, "Disc 2");
        assert_eq!(pos, "2-3");
    }

    #[test]
    fn test_no_format() {
        let (side, pos) = compute_track_labels(None, 1, Some(1), false);
        assert_eq!(side, "");
        assert_eq!(pos, "1");
    }
}
