//! A segment's stored scrub points, as playback looks them up.

use crate::db::ScrubPoint;

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
mod tests {
    use super::*;

    fn point(sample: u64, byte: u64) -> ScrubPoint {
        ScrubPoint {
            sample,
            byte,
            exact_from: sample,
        }
    }

    #[test]
    fn a_scrub_takes_the_last_point_at_or_before_its_sample() {
        let points = ScrubPoints::new(vec![
            point(441_000, 900),
            point(0, 100),
            point(882_000, 1_700),
        ]);
        assert_eq!(points.at_or_before(0), Some(point(0, 100)));
        assert_eq!(points.at_or_before(440_999), Some(point(0, 100)));
        assert_eq!(points.at_or_before(441_000), Some(point(441_000, 900)));
        assert_eq!(points.at_or_before(10_000_000), Some(point(882_000, 1_700)));
    }

    /// An MP3 point plays exactly only some frames past its own: a scrub to
    /// a sample between the two takes the point before.
    #[test]
    fn a_scrub_takes_the_last_point_that_plays_its_sample_exactly() {
        let leaning = |sample, byte| ScrubPoint {
            sample,
            byte,
            exact_from: sample + 3_456,
        };
        let points = ScrubPoints::new(vec![leaning(441_000, 900), leaning(882_000, 1_700)]);
        assert_eq!(points.at_or_before(444_455), None);
        assert_eq!(points.at_or_before(444_456), Some(leaning(441_000, 900)));
        assert_eq!(points.at_or_before(885_455), Some(leaning(441_000, 900)));
        assert_eq!(points.at_or_before(885_456), Some(leaning(882_000, 1_700)));
    }

    #[test]
    fn a_sample_before_every_point_has_none() {
        let points = ScrubPoints::new(vec![point(4_096, 100)]);
        assert_eq!(points.at_or_before(4_095), None);
    }
}
