//! What a candidate's rip artifacts prove about its medium, and whether its
//! track sheet is hashed into a disc ID because of it.

use super::*;
use crate::cue_flac::{CdRipper, CueIndex, CuePregap, CueTrack, CueTrackMode};

fn track(number: u32, start_cue_frames: u64) -> CueTrack {
    CueTrack {
        number,
        mode: CueTrackMode::Audio,
        title: Some(format!("Track {number:02}")),
        performer: None,
        indexes: vec![CueIndex {
            number: 1,
            frames: start_cue_frames,
            file_reference: "Album.flac".to_string(),
        }],
        file_reference: "Album.flac".to_string(),
        start_cue_frames,
        pregap: CuePregap::None,
        end_cue_frames: None,
    }
}

/// One file laid out as two tracks, a minute each.
fn sheet(ripper: Option<CdRipper>) -> CueSheet {
    CueSheet {
        title: Some("Album One".to_string()),
        performer: Some("Artist One".to_string()),
        catalog: None,
        date: None,
        ripper,
        tracks: vec![track(1, 0), track(2, 60 * 75)],
    }
}

fn lossless(sample_rate_hz: i64, bits: i64, channels: i64) -> AudioFormat {
    AudioFormat {
        codec: "FLAC".to_string(),
        sample_rate_hz,
        bits_per_sample: Some(bits),
        bitrate_kbps: None,
        channels,
    }
}

/// Read one sheet over one file of `format`, with `documents` beside them.
fn read_folder(sheet: &CueSheet, format: &AudioFormat, documents: &[(&Path, &str)]) -> RipReading {
    read(RipArtifacts {
        documents: documents
            .iter()
            .map(|(path, file)| RipDocument {
                path,
                file: Some(file),
            })
            .collect(),
        sheets: vec![RipSheet {
            sheet,
            audio: vec![SheetAudioDuration {
                file_reference: "Album.flac",
                duration_ms: 120_000,
            }],
            path: Path::new("/folder/Album.cue"),
            file: Some("Album.cue"),
        }],
        audio: vec![format],
    })
}

/// A sheet at a CD's rate proves nothing — a vinyl rip is often sampled at
/// one — and is hashed anyway: asking about a disc ID that matches nothing
/// costs one lookup, and one that matches proves the CD.
#[test]
fn a_bare_sheet_over_audio_at_a_cds_rate_proves_nothing_and_is_hashed() {
    let reading = read_folder(&sheet(None), &lossless(44_100, 16, 2), &[]);
    assert_eq!(reading.evidence, RipEvidence::Unproven);
    assert!(matches!(reading.disc_id, DiscIdReading::Computed(_)));
}

/// A sheet over audio no CD holds — here mono at 96 kHz, as a transfer
/// from a record is kept — rules a CD out, and is not hashed: no disc had
/// the layout it describes.
#[test]
fn a_sheet_over_audio_at_another_rate_is_not_a_cd_and_is_not_hashed() {
    let reading = read_folder(&sheet(None), &lossless(96_000, 24, 1), &[]);
    assert_eq!(
        reading.evidence,
        RipEvidence::NotCd {
            sample_rate_hz: 96_000
        }
    );
    assert_eq!(
        reading.disc_id,
        DiscIdReading::NotCdAudio {
            sample_rate_hz: 96_000
        }
    );
}

/// A sheet a CD ripper wrote proves a CD rip, and is hashed.
#[test]
fn a_sheet_a_cd_ripper_wrote_proves_a_cd() {
    let reading = read_folder(
        &sheet(Some(CdRipper::ExactAudioCopy)),
        &lossless(44_100, 16, 2),
        &[],
    );
    assert_eq!(
        reading.evidence,
        RipEvidence::Cd {
            proof: CdProof::RipperSheet,
            file: Some("Album.cue".to_string()),
        }
    );
    assert!(matches!(reading.disc_id, DiscIdReading::Computed(_)));
}

/// An AccurateRip report that found the disc proves a CD rip; one that did
/// not find it proves nothing.
#[test]
fn an_accuraterip_report_that_found_the_disc_proves_a_cd() {
    let folder = tempfile::tempdir().unwrap();
    let found = folder.path().join("found.accurip");
    std::fs::write(
        &found,
        "[CUETools log; Date: 01.01.2000 00:00:00; Version: 2.1.5]\n\
         [AccurateRip ID: 00000001-00000002-00000003] found.\n\
         Track   [  CRC   |   V2   ] Status\n",
    )
    .unwrap();
    let absent = folder.path().join("absent.accurip");
    std::fs::write(
        &absent,
        "[CUETools log; Date: 01.01.2000 00:00:00; Version: 2.1.5]\n",
    )
    .unwrap();

    let reading = read_folder(
        &sheet(None),
        &lossless(44_100, 16, 2),
        &[(&found, "found.accurip")],
    );
    assert_eq!(
        reading.evidence,
        RipEvidence::Cd {
            proof: CdProof::AccurateRipReport,
            file: Some("found.accurip".to_string()),
        }
    );
    let reading = read_folder(
        &sheet(None),
        &lossless(44_100, 16, 2),
        &[(&absent, "absent.accurip")],
    );
    assert_eq!(reading.evidence, RipEvidence::Unproven);
}

/// A rip log whose table of contents reads is the proof and the disc ID
/// both, whatever else the folder holds.
#[test]
fn a_rip_log_proves_a_cd_and_is_the_disc_id() {
    let log = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/logs/test_album.log");
    let reading = read_folder(
        &sheet(None),
        &lossless(96_000, 24, 2),
        &[(&log, "Album.log")],
    );
    assert_eq!(
        reading.evidence,
        RipEvidence::Cd {
            proof: CdProof::RipLog,
            file: Some("Album.log".to_string()),
        }
    );
    let DiscIdReading::Computed(computed) = reading.disc_id else {
        panic!("the log's table of contents hashes to a disc ID");
    };
    assert_eq!(computed.source_file.as_deref(), Some("Album.log"));
}

/// Audio that rules a CD out with no sheet to hash leaves no disc ID to
/// speak of: the reading is absent, not unhashed.
#[test]
fn audio_off_a_cds_rate_with_no_sheet_is_absent() {
    let format = lossless(96_000, 24, 2);
    let reading = read(RipArtifacts {
        documents: Vec::new(),
        sheets: Vec::new(),
        audio: vec![&format],
    });
    assert_eq!(
        reading.evidence,
        RipEvidence::NotCd {
            sample_rate_hz: 96_000
        }
    );
    assert_eq!(reading.disc_id, DiscIdReading::Absent);
}
