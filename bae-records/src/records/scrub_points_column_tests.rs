use super::*;

fn point(sample: u64, byte: u64, exact_from: u64) -> ScrubPoint {
    ScrubPoint { sample, byte, exact_from }
}

fn is_damaged(result: Result<Vec<ScrubPoint>, DbError>) -> bool {
    matches!(
        result,
        Err(DbError::Coven(error))
            if matches!(*error, coven::CovenError::Database(coven::DbError::DamagedDatabase))
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
    assert_eq!(decode(&encode(&points)).unwrap(), points);
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
    assert_eq!(encode(&points).len(), 24 * 8);
}

/// Points no maker of bae's produces are its bug: they panic, which fails
/// the write they were for.
#[test]
fn points_out_of_order_fail_the_write() {
    for points in [
        vec![point(10, 100, 10), point(10, 200, 10)],
        vec![point(10, 100, 10), point(20, 100, 20)],
        vec![point(20, 100, 20), point(10, 200, 10)],
        vec![point(20, 100, 19)],
        vec![],
    ] {
        assert!(std::panic::catch_unwind(|| encode(&points)).is_err(), "{points:?}");
    }
}

/// The table refuses a blob that holds no point or ends inside a varint;
/// one that ends a varint is left to the reader to judge.
#[test]
fn the_table_refuses_a_blob_that_ends_inside_a_varint() {
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    crate::test_fixtures::bare_library::open_bare_library(&conn);
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
    let whole = encode(&[point(5, 6, 7), point(500, 600, 700)]);
    for blob in [
        Vec::new(),
        // A point short of its third varint.
        whole[..whole.len() - 1].to_vec(),
        // A varint the blob ends inside.
        vec![0x80],
        // A varint past 64 bits.
        vec![0xff; 11],
        vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02, 0, 0],
        // A step past the largest sample.
        [encode(&[point(u64::MAX, 0, u64::MAX)]), vec![0, 0, 0]].concat(),
    ] {
        assert!(is_damaged(decode(&blob)), "{blob:?}");
    }
}
