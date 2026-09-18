use super::super::test_discs::{
    audio_disc, enhanced_cd, from_fixture, mixed_mode_cd, pattern_frames,
};
use super::*;

/// CRC-32 (the zlib polynomial) taken bit by bit, so the expected values below
/// are stated independently of the crate the kernel hashes with.
fn plain_crc32(frames: &[u32]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in frames.iter().flat_map(|frame| frame.to_le_bytes()) {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[test]
fn the_lookup_toc_lists_every_start_sector_then_the_lead_out() {
    assert_eq!(
        toc_string(&from_fixture("cueripper.log")),
        "0:49762:104737:149862:199855"
    );
}

#[test]
fn a_data_track_is_marked_in_the_lookup_toc() {
    let toc = toc_string(&mixed_mode_cd());

    assert!(toc.starts_with("-0:66578:"), "unexpected TOC string: {toc}");
    assert!(
        toc.ends_with(":261524:261826"),
        "unexpected TOC string: {toc}"
    );
}

/// Three CUETools-aware logs print the TOCID for the TOC they also print.
#[test]
fn the_tocid_matches_the_one_a_rip_log_prints() {
    for (fixture, expected) in [
        ("cueripper.log", "rNlVkfa2M2YBSqIfjvEqecBqYGE-"),
        ("test_album.log", "jedpTrBL7Crt1HOKapUurvO97_E-"),
        ("eac-ctdb.log", "6CgGdovQOYDcrOIMxNcFY5wZilc-"),
    ] {
        assert_eq!(tocid(&from_fixture(fixture)), expected, "{fixture}");
    }
}

/// An enhanced CD's TOCID ends where its audio ends, not where the disc does:
/// the final hashed slot is the audio session's lead-out (LSN 270185 — the
/// 270335 ARver's LBA listings print, less the 150-sector lead-in), while the
/// lead-out past the data track is 63466 sectors further on. Reading the data
/// track's start or the disc lead-out into that slot changes the hash.
#[test]
fn the_tocid_of_an_enhanced_cd_ends_where_its_audio_does() {
    let toc = enhanced_cd();

    assert_eq!(tocid(&toc), "6q9yYvgRUAesYZQXQ__xjqMUEmc-");
    assert_ne!(
        tocid(&toc),
        tocid(&audio_disc(
            &toc.audio_tracks()
                .map(|track| track.start)
                .collect::<Vec<_>>(),
            toc.leadout()
        ))
    );
}

/// The TOCID is taken relative to the first audio track, so the same audio
/// pressed at a different place on the disc hashes the same.
#[test]
fn the_tocid_is_relative_to_the_first_audio_track() {
    let at_zero = audio_disc(&[0, 30_000, 70_000], 120_000);
    let shifted = audio_disc(&[500, 30_500, 70_500], 120_500);

    assert_eq!(tocid(&at_zero), tocid(&shifted));
}

#[test]
fn the_disc_checksum_drops_a_block_at_each_end() {
    let disc = pattern_frames(4 * 5_880 + 1_234);
    let tail = 5_880 + disc.len() % 5_880;

    assert_eq!(
        disc_crc32(&disc),
        Some(plain_crc32(&disc[5_880..disc.len() - tail]))
    );
}

#[test]
fn a_disc_shorter_than_the_drops_has_no_checksum() {
    assert_eq!(disc_crc32(&pattern_frames(6_000)), None);
}

#[test]
fn a_middle_track_is_checksummed_whole() {
    let disc = pattern_frames(60_000);
    let track = 20_000..40_000;

    assert_eq!(
        track_crc32(&disc, track.clone(), TrackEdges::MIDDLE),
        Some(plain_crc32(&disc[track]))
    );
}

#[test]
fn the_first_track_drops_the_discs_head_block() {
    let disc = pattern_frames(60_000);
    let edges = TrackEdges {
        first: true,
        last: false,
    };

    assert_eq!(
        track_crc32(&disc, 0..20_000, edges),
        Some(plain_crc32(&disc[5_880..20_000]))
    );
}

#[test]
fn the_last_track_drops_the_discs_tail_block() {
    let disc = pattern_frames(60_000);
    let edges = TrackEdges {
        first: false,
        last: true,
    };
    let tail = 5_880 + disc.len() % 5_880;

    assert_eq!(
        track_crc32(&disc, 40_000..60_000, edges),
        Some(plain_crc32(&disc[40_000..60_000 - tail]))
    );
}

#[test]
fn a_track_shorter_than_the_drops_has_no_checksum() {
    let disc = pattern_frames(60_000);
    let edges = TrackEdges {
        first: true,
        last: false,
    };

    assert_eq!(track_crc32(&disc, 0..5_000, edges), None);
}
