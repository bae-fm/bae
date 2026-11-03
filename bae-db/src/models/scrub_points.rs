//! A segment's stored scrub points, as a decode looks them up.

use super::ScrubPoint;

/// The frames a scrub can start its decode at in one segment's file, in
/// sample order. Import stores them for a FLAC whose seektable is missing or
/// sparser than their spacing and for every MP3; a scrub in any other file
/// seeks by sample through the demuxer's own index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrubPoints(Vec<ScrubPoint>);

impl ScrubPoints {
    /// The points, in sample order. A later frame plays exactly no earlier
    /// than an earlier one, so that is also their order by `exact_from`.
    pub fn new(mut points: Vec<ScrubPoint>) -> Self {
        points.sort_by_key(|point| point.sample);
        Self(points)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_slice(&self) -> &[ScrubPoint] {
        &self.0
    }

    /// The last point a decode from which plays `sample` exactly: where a
    /// scrub to `sample` starts, then decodes forward to it.
    pub fn at_or_before(&self, sample: u64) -> Option<ScrubPoint> {
        let after = self.0.partition_point(|point| point.exact_from <= sample);
        after.checked_sub(1).map(|index| self.0[index])
    }
}

#[cfg(test)]
#[path = "scrub_points_tests.rs"]
mod tests;
