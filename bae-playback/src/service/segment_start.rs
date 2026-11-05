//! Where a decode inside one of a track's segments starts reading its file.
//!
//! An MP3's frames state no position and lean on the frames before them, so
//! neither a byte jump nor a sample seek puts out its own samples: any start,
//! a track's first included, decodes from the last stored point that plays
//! the target exactly, or, before the first such point, from the stream's
//! start.
//!
//! Any other seek into a segment with stored scrub points (a FLAC the demuxer
//! can only binary-search) jumps by byte to the last one at or before its
//! target; any other start reads from what the segment's span records
//! ([`SegmentStart::Span`]).

use bae_records::ScrubPoints;
use bae_track_audio::SegmentStart;
use bae_util::content_type::ContentType;
use tracing::warn;

/// Where a decode `offset` samples into a segment of a `content_type` file
/// starts, the segment beginning at sample `segment_start` of the file with
/// `scrub_points` stored for it. `file_id` names the file in the log.
pub(super) fn segment_start(
    content_type: &ContentType,
    scrub_points: &ScrubPoints,
    segment_start: u64,
    offset: u64,
    file_id: &str,
) -> SegmentStart {
    let target = segment_start + offset;
    if *content_type == ContentType::Mp3 {
        return match scrub_points.at_or_before(target) {
            Some(point) => SegmentStart::ScrubPoint(point),
            None => SegmentStart::StreamStart,
        };
    }
    if offset == 0 || scrub_points.is_empty() {
        return SegmentStart::Span;
    }
    match scrub_points.at_or_before(target) {
        Some(point) => SegmentStart::ScrubPoint(point),
        None => {
            // Import's first point is the frame the segment starts in, so
            // every sample of the segment has one at or before it.
            warn!(
                file_id,
                target, "no stored scrub point at or before the seek target; seeking by sample"
            );
            SegmentStart::Span
        }
    }
}

#[cfg(test)]
#[path = "segment_start_tests.rs"]
mod tests;
