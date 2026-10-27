//! A segment's scrub points as the one `points` blob that keeps them: each
//! point, in sample order, three unsigned LEB128 varints -- how far its
//! sample is past the point before's, less one (the first point: its
//! sample); the same for its byte; and how far its `exact_from` is past its
//! sample. A step less one can't spell a point at or before the one before
//! it, so every blob that decodes is points in order.
//!
//! One writer and one reader. A blob that doesn't decode to whole points is a
//! damaged database, never read as fewer or other points.

use crate::db::{DbError, ScrubPoint};

/// `points` as their blob. Points not each past the one before, or one
/// whose `exact_from` is before its own sample, are a bug in their maker:
/// they fail the write.
#[cfg(any(test, feature = "test-utils", desktop))]
pub(super) fn encode(points: &[ScrubPoint]) -> Result<Vec<u8>, DbError> {
    if points.is_empty() {
        return Err(DbError::Message(
            "a segment with no scrub points stores no row".to_string(),
        ));
    }
    let mut blob = Vec::with_capacity(points.len() * 8);
    let mut before: Option<ScrubPoint> = None;
    for point in points {
        let (sample, byte) = match before {
            None => (point.sample, point.byte),
            Some(before) => (
                step(before.sample, point.sample, "sample", points)?,
                step(before.byte, point.byte, "byte", points)?,
            ),
        };
        let lead = point.exact_from.checked_sub(point.sample).ok_or_else(|| {
            DbError::Message(format!(
                "scrub point {point:?} plays exactly from before its own sample"
            ))
        })?;
        for value in [sample, byte, lead] {
            push_varint(&mut blob, value);
        }
        before = Some(*point);
    }
    Ok(blob)
}

/// How far `to` is past `from`, less one.
#[cfg(any(test, feature = "test-utils", desktop))]
fn step(from: u64, to: u64, what: &str, points: &[ScrubPoint]) -> Result<u64, DbError> {
    to.checked_sub(from)
        .and_then(|step| step.checked_sub(1))
        .ok_or_else(|| DbError::Message(format!("scrub points out of {what} order: {points:?}")))
}

#[cfg(any(test, feature = "test-utils", desktop))]
fn push_varint(blob: &mut Vec<u8>, mut value: u64) {
    loop {
        let low = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            blob.push(low);
            return;
        }
        blob.push(low | 0x80);
    }
}

/// The points a blob holds, in sample order.
pub(super) fn decode(blob: &[u8]) -> Result<Vec<ScrubPoint>, DbError> {
    let damaged = || DbError::from(coven::DbError::DamagedDatabase);
    let mut values = Varints(blob);
    let mut points: Vec<ScrubPoint> = Vec::with_capacity(blob.len() / 8);
    while !values.0.is_empty() {
        let mut next = || values.next().ok_or_else(damaged);
        let (sample, byte, lead) = (next()?, next()?, next()?);
        let (sample, byte) = match points.last() {
            None => (sample, byte),
            Some(before) => (
                past(before.sample, sample).ok_or_else(damaged)?,
                past(before.byte, byte).ok_or_else(damaged)?,
            ),
        };
        points.push(ScrubPoint {
            sample,
            byte,
            exact_from: sample.checked_add(lead).ok_or_else(damaged)?,
        });
    }
    if points.is_empty() {
        return Err(damaged());
    }
    Ok(points)
}

/// The value `step` plus one past `from`; `None` past `u64::MAX`.
fn past(from: u64, step: u64) -> Option<u64> {
    from.checked_add(step)?.checked_add(1)
}

/// The varints a blob spells, one at a time. `None` for a varint the blob
/// ends inside, or one too large for a `u64`.
struct Varints<'a>(&'a [u8]);

impl Varints<'_> {
    fn next(&mut self) -> Option<u64> {
        let mut value = 0u64;
        for (index, &byte) in self.0.iter().enumerate() {
            let shift = 7 * index as u32;
            let bits = u64::from(byte & 0x7f);
            if shift >= 64 || (bits << shift) >> shift != bits {
                return None;
            }
            value |= bits << shift;
            if byte & 0x80 == 0 {
                self.0 = &self.0[index + 1..];
                return Some(value);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(sample: u64, byte: u64, exact_from: u64) -> ScrubPoint {
        ScrubPoint {
            sample,
            byte,
            exact_from,
        }
    }

    fn is_damaged(result: Result<Vec<ScrubPoint>, DbError>) -> bool {
        matches!(
            result,
            Err(DbError::Coven(coven::CovenError::Database(
                coven::DbError::DamagedDatabase
            )))
        )
    }

    #[test]
    fn points_read_back_as_written() {
        let points = vec![
            point(0, 8_288, 0),
            point(436_655, 159_286, 440_111),
            point(436_656, 159_287, 436_656),
            point(u64::MAX - 1, u64::MAX - 1, u64::MAX),
        ];
        assert_eq!(decode(&encode(&points).unwrap()).unwrap(), points);
    }

    /// An MP3's points 10 seconds apart at 320 kbps take a few bytes each:
    /// a step of 441,000 samples, 400,000 bytes, and a lead of a few frames.
    #[test]
    fn an_mp3s_points_take_eight_bytes_each() {
        let points: Vec<ScrubPoint> = (1..=24u64)
            .map(|index| {
                let sample = index * 441_000;
                point(sample, index * 400_000, sample + 3_456)
            })
            .collect();
        assert_eq!(encode(&points).unwrap().len(), 24 * 8);
    }

    #[test]
    fn points_out_of_order_fail_the_write() {
        for points in [
            vec![point(10, 100, 10), point(10, 200, 10)],
            vec![point(10, 100, 10), point(20, 100, 20)],
            vec![point(20, 100, 20), point(10, 200, 10)],
            vec![point(20, 100, 19)],
            vec![],
        ] {
            assert!(
                matches!(encode(&points), Err(DbError::Message(_))),
                "{points:?}"
            );
        }
    }

    /// The table refuses a blob that holds no point or ends inside a varint;
    /// one that ends a varint is left to the reader to judge.
    #[test]
    fn the_table_refuses_a_blob_that_ends_inside_a_varint() {
        let conn = crate::library::bare_library_connection();
        conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
        let insert = |blob: &[u8]| {
            conn.execute(
                "INSERT OR REPLACE INTO audio_segment_scrub_points \
                 (audio_format_segment_id, points) VALUES ('segment', ?)",
                rusqlite::params![blob],
            )
        };
        for refused in [&[][..], &[0, 0], &[0, 0, 0x80]] {
            assert!(insert(refused).is_err(), "{refused:?}");
        }
        assert!(insert(&[0, 0, 0]).is_ok());
        assert!(insert(&[0, 0, 0, 0]).is_ok());
        assert!(is_damaged(decode(&[0, 0, 0, 0])));
    }

    #[test]
    fn a_blob_that_is_not_whole_points_is_a_damaged_database() {
        let whole = encode(&[point(5, 6, 7), point(500, 600, 700)]).unwrap();
        for blob in [
            Vec::new(),
            // A point short of its third varint.
            whole[..whole.len() - 1].to_vec(),
            // A varint the blob ends inside.
            vec![0x80],
            // A varint past 64 bits.
            vec![0xff; 11],
            vec![
                0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02, 0, 0,
            ],
            // A step past the largest sample.
            [
                encode(&[point(u64::MAX, 0, u64::MAX)]).unwrap(),
                vec![0, 0, 0],
            ]
            .concat(),
        ] {
            assert!(is_damaged(decode(&blob)), "{blob:?}");
        }
    }
}
