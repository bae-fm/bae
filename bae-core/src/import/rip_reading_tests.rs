//! What a candidate's rip artifacts prove about its medium, and whether its
//! track sheet is hashed into a disc ID because of it.

use super::*;
use crate::cue_flac::{CdRipper, CueIndex, CueTrack, CueTrackMode};

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
        generated_pregap_frames: None,
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
    assert_eq!(reading.origin, AudioOrigin::default());
    assert!(matches!(reading.disc_id, DiscIdReading::Computed(_)));
}

/// A sheet over audio no CD holds — here mono at 96 kHz, as a transfer
/// from a record is kept — rules a CD out, and is not hashed: no disc had
/// the layout it describes.
#[test]
fn a_sheet_over_audio_at_another_rate_is_not_a_cd_and_is_not_hashed() {
    let reading = read_folder(&sheet(None), &lossless(96_000, 24, 1), &[]);
    assert_eq!(
        reading.origin,
        AudioOrigin {
            source: None,
            not_cd_rate: Some(96_000),
        }
    );
    assert_eq!(reading.disc_id, DiscIdReading::NotCdAudio);
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
        reading.origin.source,
        Some(AudioSource::CdRip {
            proof: CdProof::RipperSheet,
            file: Some("Album.cue".to_string()),
        })
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
        reading.origin.source,
        Some(AudioSource::CdRip {
            proof: CdProof::AccurateRipReport,
            file: Some("found.accurip".to_string()),
        })
    );
    let reading = read_folder(
        &sheet(None),
        &lossless(44_100, 16, 2),
        &[(&absent, "absent.accurip")],
    );
    assert_eq!(reading.origin, AudioOrigin::default());
}

/// A rip log whose table of contents reads is the proof and the disc ID
/// both, and the sheet beside it, which lays out the same disc, is not
/// hashed again.
#[test]
fn a_rip_log_proves_a_cd_and_is_the_disc_id() {
    let log = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/logs/test_album.log");
    let sheet = sheet(None);
    let format = lossless(96_000, 24, 2);
    let reading = read(RipArtifacts {
        documents: vec![RipDocument {
            path: &log,
            file: Some("Album.log"),
        }],
        sheets: vec![RipSheet {
            sheet: &sheet,
            audio: vec![SheetAudioDuration {
                file_reference: "Album.flac",
                duration_ms: 120_000,
            }],
            path: &log.with_file_name("Album.cue"),
            file: Some("Album.cue"),
        }],
        audio: vec![&format],
    });
    assert_eq!(
        reading.origin,
        AudioOrigin {
            source: Some(AudioSource::CdRip {
                proof: CdProof::RipLog,
                file: Some("Album.log".to_string()),
            }),
            not_cd_rate: Some(96_000),
        },
        "the rate is read beside the proof"
    );
    let DiscIdReading::Computed(discs) = reading.disc_id else {
        panic!("the log's table of contents hashes to a disc ID");
    };
    assert_eq!(
        discs
            .iter()
            .map(|disc| disc.source_file.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("Album.log")]
    );
}

/// A rip log's table of contents with tracks of the given sector lengths.
fn log_text(track_sectors: &[i32]) -> String {
    let mut text = String::from(
        "TOC of the extracted CD\n\n     Track |   Start  |  Length  | Start sector | End sector\n    -----------------------------------------------------------\n",
    );
    let mut start = 0;
    for (index, sectors) in track_sectors.iter().enumerate() {
        text.push_str(&format!(
            "        {}  |  0:00.00 |  0:00.00 |    {start}    |    {}\n",
            index + 1,
            start + sectors - 1
        ));
        start += sectors;
    }
    text.push('\n');
    text
}

/// A folder holding one rip per disc — a log and a sheet in each disc's
/// directory — computes one disc ID per disc, each from its own log.
#[test]
fn a_folder_with_a_log_per_disc_computes_every_disc_id() {
    let folder = tempfile::tempdir().unwrap();
    let mut logs = Vec::new();
    for (disc, sectors) in [("Disc 1", [18_000, 13_500]), ("Disc 2", [21_000, 9_000])] {
        let directory = folder.path().join(disc);
        std::fs::create_dir(&directory).unwrap();
        let log = directory.join("Album.log");
        std::fs::write(&log, log_text(&sectors)).unwrap();
        logs.push((log, format!("{disc}/Album.log")));
    }
    let sheet = sheet(None);
    let format = lossless(44_100, 16, 2);
    let sheet_paths: Vec<_> = logs
        .iter()
        .map(|(log, _)| log.with_file_name("Album.cue"))
        .collect();
    let reading = read(RipArtifacts {
        documents: logs
            .iter()
            .map(|(path, file)| RipDocument {
                path,
                file: Some(file),
            })
            .collect(),
        sheets: sheet_paths
            .iter()
            .map(|path| RipSheet {
                sheet: &sheet,
                audio: vec![SheetAudioDuration {
                    file_reference: "Album.flac",
                    duration_ms: 120_000,
                }],
                path,
                file: Some("Album.cue"),
            })
            .collect(),
        audio: vec![&format],
    });

    let DiscIdReading::Computed(discs) = reading.disc_id else {
        panic!("each log hashes to a disc ID");
    };
    assert_eq!(
        discs
            .iter()
            .map(|disc| disc.source_file.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("Disc 1/Album.log"), Some("Disc 2/Album.log")],
        "one disc ID per log, and no sheet beside a log is hashed"
    );
    assert_ne!(discs[0].disc_id, discs[1].disc_id);
}

/// Two sheets of one disc hash to one disc ID, which is kept once; a sheet
/// of another disc, with no log beside it, adds its own.
#[test]
fn every_sheet_without_a_log_is_hashed_and_one_disc_is_kept_once() {
    let one = sheet(None);
    let other = CueSheet {
        tracks: vec![track(1, 0), track(2, 90 * 75), track(3, 150 * 75)],
        ..sheet(None)
    };
    let format = lossless(44_100, 16, 2);
    let rip_sheet = |sheet, path, file| RipSheet {
        sheet,
        audio: vec![SheetAudioDuration {
            file_reference: "Album.flac",
            duration_ms: 200_000,
        }],
        path,
        file: Some(file),
    };
    let reading = read(RipArtifacts {
        documents: Vec::new(),
        sheets: vec![
            rip_sheet(
                &one,
                Path::new("/folder/Disc 1/Album.cue"),
                "Disc 1/Album.cue",
            ),
            rip_sheet(
                &one,
                Path::new("/folder/Disc 1/Copy.cue"),
                "Disc 1/Copy.cue",
            ),
            rip_sheet(
                &other,
                Path::new("/folder/Disc 2/Album.cue"),
                "Disc 2/Album.cue",
            ),
        ],
        audio: vec![&format],
    });
    let DiscIdReading::Computed(discs) = reading.disc_id else {
        panic!("the sheets hash to disc IDs");
    };
    assert_eq!(
        discs
            .iter()
            .map(|disc| disc.source_file.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("Disc 1/Album.cue"), Some("Disc 2/Album.cue")]
    );
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
    assert_eq!(reading.origin.not_cd_rate, Some(96_000));
    assert_eq!(reading.disc_id, DiscIdReading::Absent);
}
