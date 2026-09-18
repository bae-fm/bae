use super::super::test_discs::{enhanced_cd, from_fixture, mixed_mode_cd};
use super::*;

/// The disc id a CUERipper log prints as the third part of its AccurateRip id.
#[test]
fn matches_the_id_a_rip_log_prints() {
    // `[AccurateRip ID: 0007b198-001eb52b-2a0a6804]`
    assert_eq!(
        format!("{:08x}", cddb_disc_id(&from_fixture("cueripper.log"))),
        "2a0a6804"
    );
}

/// EAC printed no freedb id in this log, so the expected value is computed by
/// hand from the TOC table it does print: ten tracks starting at sectors 0,
/// 23410, 47255, 63420, 79335, 98402, 117585, 133352, 151277 and 169722, with
/// the lead-out at 188815. Digit sums of each start second give 0x7f as the
/// checksum, the disc plays 2517 seconds (0x09d5), and 10 tracks is 0x0a.
#[test]
fn matches_a_hand_computed_id() {
    assert_eq!(
        format!("{:08x}", cddb_disc_id(&from_fixture("test_album.log"))),
        "7f09d50a"
    );
}

/// An enhanced CD's data track counts: it lengthens the disc by its own 694
/// seconds plus the 152-second session gap, and raises the track count to 11.
/// ARver's `doc/data_track.md` gives both ids for this layout.
#[test]
fn counts_a_trailing_data_track() {
    assert_eq!(format!("{:08x}", cddb_disc_id(&enhanced_cd())), "9e11600b");
}

#[test]
fn counts_a_leading_data_track() {
    assert_eq!(
        format!("{:08x}", cddb_disc_id(&mixed_mode_cd())),
        "af0da31d"
    );
}
