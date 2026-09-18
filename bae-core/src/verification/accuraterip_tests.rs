use super::super::test_discs::{enhanced_cd, from_fixture, mixed_mode_cd, pattern_frames};
use super::*;

/// AccurateRip's checksums written out plainly: every sample of `track`
/// weighted by its position, counting from `first_position`. No skips — the
/// tests take those back out by hand, so this stays an independent statement
/// of the formula rather than a second copy of the kernel.
fn plain_checksums(track: &[u32], first_position: u64) -> TrackChecksums {
    let mut v1 = 0u32;
    let mut high = 0u32;
    for (index, sample) in track.iter().enumerate() {
        let product = u64::from(*sample) * (first_position + index as u64);
        v1 = v1.wrapping_add(product as u32);
        high = high.wrapping_add((product >> 32) as u32);
    }
    TrackChecksums {
        v1,
        v2: v1.wrapping_add(high),
    }
}

/// What is left of `whole` once `part`'s samples are taken out. Both schemes
/// are sums, so removing a run of samples is subtraction.
fn without(whole: TrackChecksums, part: TrackChecksums) -> TrackChecksums {
    TrackChecksums {
        v1: whole.v1.wrapping_sub(part.v1),
        v2: whole.v2.wrapping_sub(part.v2),
    }
}

#[test]
fn ids_match_the_ones_a_rip_log_prints() {
    // `[AccurateRip ID: 0007b198-001eb52b-2a0a6804]`
    let ids = AccurateRipIds::of(&from_fixture("cueripper.log"));

    assert_eq!(
        (
            format!("{:08x}", ids.id1),
            format!("{:08x}", ids.id2),
            format!("{:08x}", ids.id3)
        ),
        (
            "0007b198".to_string(),
            "001eb52b".to_string(),
            "2a0a6804".to_string()
        )
    );
}

/// ARver's `doc/data_track.md` publishes the ids dBpoweramp and EAC compute for
/// an enhanced CD: the data track stays out of the two sector sums, but the
/// lead-out past it is what they are taken against.
#[test]
fn a_trailing_data_track_stays_out_of_the_sums_but_moves_the_lead_out() {
    let toc = enhanced_cd();
    let ids = AccurateRipIds::of(&toc);

    assert_eq!(
        ids.dbar_path(toc.audio_track_count()),
        "accuraterip/9/1/4/dBAR-010-00164419-00b9f6e2-9e11600b.bin"
    );
}

/// The same document's mixed mode CD: the data track comes first, the audio is
/// numbered from 1 regardless, and the record path counts 28 audio tracks out
/// of the disc's 29.
#[test]
fn a_leading_data_track_leaves_the_audio_numbered_from_one() {
    let toc = mixed_mode_cd();
    let ids = AccurateRipIds::of(&toc);

    assert_eq!(
        ids.dbar_path(toc.audio_track_count()),
        "accuraterip/4/5/a/dBAR-028-00517a54-05a845d2-af0da31d.bin"
    );
}

#[test]
fn a_middle_tracks_checksums_weigh_every_sample() {
    let disc = pattern_frames(24_000);
    let track = 8_000..16_000;

    assert_eq!(
        track_checksums(&disc, track.clone(), TrackEdges::MIDDLE, 0),
        Some(plain_checksums(&disc[track], 1))
    );
}

#[test]
fn the_first_track_drops_the_samples_before_position_2940() {
    let disc = pattern_frames(24_000);
    let track = 0..8_000;
    let edges = TrackEdges {
        first: true,
        last: false,
    };

    let whole = plain_checksums(&disc[track.clone()], 1);
    let head = plain_checksums(&disc[0..2_939], 1);

    assert_eq!(
        track_checksums(&disc, track, edges, 0),
        Some(without(whole, head))
    );
}

#[test]
fn the_last_track_drops_its_final_2940_samples() {
    let disc = pattern_frames(24_000);
    let track = 16_000..24_000;
    let edges = TrackEdges {
        first: false,
        last: true,
    };

    let whole = plain_checksums(&disc[track.clone()], 1);
    let tail = plain_checksums(&disc[24_000 - 2_940..24_000], 8_000 - 2_940 + 1);

    assert_eq!(
        track_checksums(&disc, track, edges, 0),
        Some(without(whole, tail))
    );
}

#[test]
fn a_one_track_disc_drops_both_ends() {
    let disc = pattern_frames(12_000);
    let edges = TrackEdges {
        first: true,
        last: true,
    };

    let whole = plain_checksums(&disc, 1);
    let head = plain_checksums(&disc[0..2_939], 1);
    let tail = plain_checksums(&disc[12_000 - 2_940..12_000], 12_000 - 2_940 + 1);

    assert_eq!(
        track_checksums(&disc, 0..12_000, edges, 0),
        Some(without(without(whole, head), tail))
    );
}

#[test]
fn a_track_shorter_than_its_skips_has_no_checksum() {
    let disc = pattern_frames(4_000);
    let edges = TrackEdges {
        first: true,
        last: true,
    };

    assert_eq!(track_checksums(&disc, 0..4_000, edges, 0), None);
}

#[test]
fn an_offset_slides_the_window_into_the_neighbouring_track() {
    let disc = pattern_frames(24_000);
    let track = 8_000..16_000;

    // The samples a pressing 12 ahead would have put in this track, weighted
    // from the same first position.
    assert_eq!(
        track_checksums(&disc, track, TrackEdges::MIDDLE, 12),
        Some(plain_checksums(&disc[8_012..16_012], 1))
    );
}

#[test]
fn a_slide_that_leaves_the_disc_has_no_checksum() {
    let disc = pattern_frames(24_000);

    assert_eq!(
        track_checksums(&disc, 16_000..24_000, TrackEdges::MIDDLE, 1),
        None
    );
    assert_eq!(
        track_checksums(&disc, 0..8_000, TrackEdges::MIDDLE, -1),
        None
    );
}

/// One sector deep in the track, each sample weighted by its place in that
/// sector — computed here straight from the plan's window rather than through
/// the kernel's bounds handling.
#[test]
fn the_offset_finding_checksum_covers_sector_450() {
    let disc = pattern_frames(451 * SAMPLES_PER_SECTOR + 8_000);
    let expected = disc[450 * SAMPLES_PER_SECTOR..451 * SAMPLES_PER_SECTOR]
        .iter()
        .enumerate()
        .fold(0u32, |crc, (index, sample)| {
            crc.wrapping_add(sample.wrapping_mul(index as u32 + 1))
        });

    assert_eq!(crc450(&disc, 0, 0), Some(expected));
}

#[test]
fn a_track_that_never_reaches_sector_450_has_no_offset_finding_checksum() {
    let disc = pattern_frames(400 * SAMPLES_PER_SECTOR);

    assert_eq!(crc450(&disc, 0, 0), None);
}

#[test]
fn the_search_finds_the_pressing_that_shifted_twelve_samples() {
    let disc = pattern_frames(451 * SAMPLES_PER_SECTOR + 8_000);
    let record = crc450(&disc, 0, 12).expect("sector 450 is on this disc");

    assert_eq!(find_pressing_offset(&disc, 0, record), Some(12));
    // The offset is what the full checksum is then taken at.
    assert_eq!(
        track_checksums(&disc, 0..disc.len() - 4_000, TrackEdges::MIDDLE, 12),
        Some(plain_checksums(&disc[12..disc.len() - 3_988], 1))
    );
}

#[test]
fn an_unshifted_pressing_wins_over_a_distant_coincidence() {
    let disc = pattern_frames(451 * SAMPLES_PER_SECTOR + 8_000);
    let record = crc450(&disc, 0, 0).expect("sector 450 is on this disc");

    assert_eq!(find_pressing_offset(&disc, 0, record), Some(0));
}

#[test]
fn a_checksum_no_pressing_produces_finds_no_offset() {
    let disc = pattern_frames(451 * SAMPLES_PER_SECTOR + 8_000);

    assert_eq!(find_pressing_offset(&disc, 0, 0xDEAD_BEEF), None);
}

#[test]
fn the_search_covers_accuraterips_whole_offset_range() {
    let disc = pattern_frames(451 * SAMPLES_PER_SECTOR + 2 * MAX_PRESSING_OFFSET as usize);
    let widest = crc450(&disc, 0, MAX_PRESSING_OFFSET).expect("the widest offset is on this disc");

    assert_eq!(
        find_pressing_offset(&disc, 0, widest),
        Some(MAX_PRESSING_OFFSET)
    );
}
