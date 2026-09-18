use super::*;
use std::path::PathBuf;

/// A WAV file carrying exactly these stereo frames, at the format named.
fn wav(frames: &[(i16, i16)], sample_rate: u32, channels: u16, bits: u16) -> Vec<u8> {
    let mut data = Vec::with_capacity(frames.len() * 4);
    for (left, right) in frames {
        for sample in [left, right].into_iter().take(channels as usize) {
            match bits {
                16 => data.extend_from_slice(&sample.to_le_bytes()),
                24 => {
                    let wide = i32::from(*sample) << 8;
                    data.extend_from_slice(&wide.to_le_bytes()[..3]);
                }
                other => panic!("the fixture writer has no {other}-bit form"),
            }
        }
    }
    let block_align = channels * bits / 8;
    let mut file = Vec::with_capacity(44 + data.len());
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(b"fmt ");
    file.extend_from_slice(&16u32.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&channels.to_le_bytes());
    file.extend_from_slice(&sample_rate.to_le_bytes());
    file.extend_from_slice(&(sample_rate * u32::from(block_align)).to_le_bytes());
    file.extend_from_slice(&block_align.to_le_bytes());
    file.extend_from_slice(&bits.to_le_bytes());
    file.extend_from_slice(b"data");
    file.extend_from_slice(&(data.len() as u32).to_le_bytes());
    file.extend_from_slice(&data);
    file
}

/// Stereo frames that differ from one another, so a misordered read shows up.
fn varied_frames(count: usize, seed: u32) -> Vec<(i16, i16)> {
    let mut state = seed;
    (0..count)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((state >> 16) as i16, state as i16)
        })
        .collect()
}

/// The frame a pair of samples packs into: right in the high half, left in the
/// low half, which is the little-endian PCM word read as a `u32`.
fn packed(frames: &[(i16, i16)]) -> Vec<u32> {
    frames
        .iter()
        .map(|(left, right)| (u32::from(*right as u16) << 16) | u32::from(*left as u16))
        .collect()
}

/// Write each file into a fresh directory and hand back its paths in order.
fn rip(files: &[Vec<u8>]) -> (tempfile::TempDir, Vec<PathBuf>) {
    crate::audio_codec::init();
    let dir = tempfile::tempdir().expect("a temp dir for the rip");
    let paths = files
        .iter()
        .enumerate()
        .map(|(index, bytes)| {
            let path = dir.path().join(format!("{:02}.wav", index + 1));
            std::fs::write(&path, bytes).expect("writing a rip fixture");
            path
        })
        .collect();
    (dir, paths)
}

#[test]
fn reads_a_rips_tracks_end_to_end() {
    let one = varied_frames(4_410, 1);
    let two = varied_frames(2_205, 2);
    let (_dir, paths) = rip(&[wav(&one, 44_100, 2, 16), wav(&two, 44_100, 2, 16)]);

    let disc = DiscSamples::read(&paths).expect("a 44.1 kHz 16-bit stereo rip reads");

    let mut expected = packed(&one);
    expected.extend(packed(&two));
    assert_eq!(disc.frames(), expected.as_slice());
    assert_eq!(disc.track_count(), 2);
    assert_eq!(disc.track(0), Some(0..4_410));
    assert_eq!(disc.track(1), Some(4_410..6_615));
    assert_eq!(disc.track(2), None);
}

#[test]
fn packs_the_stored_sample_values_verbatim() {
    let mut frames = vec![(0, 0), (1, 2), (-1, -2), (i16::MAX, i16::MIN)];
    frames.extend(varied_frames(4_410, 3));
    let (_dir, paths) = rip(&[wav(&frames, 44_100, 2, 16)]);

    let disc = DiscSamples::read(&paths).expect("a CD-audio WAV reads");

    assert_eq!(
        &disc.frames()[..4],
        &[0x0000_0000, 0x0002_0001, 0xFFFE_FFFF, 0x8000_7FFF]
    );
    assert_eq!(disc.frames(), packed(&frames).as_slice());
}

#[test]
fn the_first_and_last_tracks_are_the_discs_edges() {
    let (_dir, paths) = rip(&[
        wav(&varied_frames(4_410, 1), 44_100, 2, 16),
        wav(&varied_frames(4_410, 2), 44_100, 2, 16),
        wav(&varied_frames(4_410, 3), 44_100, 2, 16),
    ]);

    let disc = DiscSamples::read(&paths).expect("a CD-audio rip reads");

    assert_eq!(
        disc.edges(0),
        TrackEdges {
            first: true,
            last: false
        }
    );
    assert_eq!(disc.edges(1), TrackEdges::MIDDLE);
    assert_eq!(
        disc.edges(2),
        TrackEdges {
            first: false,
            last: true
        }
    );
}

#[test]
fn a_one_track_rip_is_both_edges() {
    let (_dir, paths) = rip(&[wav(&varied_frames(4_410, 1), 44_100, 2, 16)]);

    let disc = DiscSamples::read(&paths).expect("a CD-audio rip reads");

    assert_eq!(
        disc.edges(0),
        TrackEdges {
            first: true,
            last: true
        }
    );
}

#[test]
fn audio_that_did_not_come_off_a_cd_is_refused() {
    let frames = varied_frames(4_410, 1);
    for (name, bytes) in [
        ("48 kHz", wav(&frames, 48_000, 2, 16)),
        ("mono", wav(&frames, 44_100, 1, 16)),
        ("24-bit", wav(&frames, 44_100, 2, 24)),
    ] {
        let (_dir, paths) = rip(&[bytes]);

        let error = DiscSamples::read(&paths)
            .err()
            .unwrap_or_else(|| panic!("{name} is not CD audio"));
        assert!(
            matches!(error, VerificationError::NotCdAudio { .. }),
            "{name}: unexpected error {error}"
        );
    }
}

#[test]
fn a_rip_with_no_files_has_nothing_to_read() {
    let error = DiscSamples::read(&[])
        .err()
        .expect("an empty rip has nothing to read");

    assert_eq!(error, VerificationError::NoAudio);
}
