use super::*;
use std::path::Path;

/// Read a checked-in log the way the import pass will: off disk, through the
/// encoding detection, since EAC writes UTF-16LE by default.
fn log(name: &str) -> RipLog {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/logs")
        .join(name);
    let text = crate::text_encoding::read_text_file(&path)
        .expect("log fixture should be readable")
        .text;
    parse_rip_log(&text).expect("log fixture should parse")
}

fn track(log: &RipLog, number: u32) -> &TrackResult {
    log.tracks
        .iter()
        .find(|track| track.number == number)
        .unwrap_or_else(|| panic!("log should carry track {number}"))
}

fn matched_copies(log: &RipLog) -> Option<u32> {
    Verification::of(log).matched_copies()
}

#[test]
fn eac_range_rip_reads_the_accuraterip_summary_and_the_ctdb_table() {
    let log = log("test_album.log");

    assert_eq!(
        log.ripper,
        Ripper::Eac {
            version: "1.6".to_string()
        }
    );
    assert_eq!(log.tracks.len(), 10);
    // A range rip writes one file for the whole disc, so the per-track results
    // exist only in the two summaries.
    assert_eq!(
        track(&log, 1).accuraterip,
        AccurateRipTrack::Matched {
            confidence: 15,
            version: ArVersion::V2,
            crc: Some(0xAD63EDEB),
        }
    );
    assert_eq!(track(&log, 1).copy_crc, None);
    assert_eq!(
        track(&log, 8).ctdb,
        Some(CtdbTrack::Matched {
            confidence: 299,
            total: 318,
        })
    );
    assert_eq!(
        log.accuraterip,
        Some(AccurateRipSummary {
            disc_ids: None,
            in_database: true,
        })
    );
    assert_eq!(
        log.ctdb,
        Some(CtdbSummary {
            tocid: "jedpTrBL7Crt1HOKapUurvO97_E-".to_string(),
            in_database: true,
        })
    );
    // The weakest track is the one 299 CUETools copies confirm.
    assert_eq!(matched_copies(&log), Some(299));
}

#[test]
fn eac_0_99_reads_matches_that_name_no_accuraterip_version() {
    let log = log("eac-0.99.log");

    assert_eq!(
        log.ripper,
        Ripper::Eac {
            version: "0.99 prebeta 5".to_string()
        }
    );
    assert_eq!(log.tracks.len(), 13);
    assert_eq!(
        *track(&log, 1),
        TrackResult {
            number: 1,
            test_crc: Some(0xFD6B2546),
            copy_crc: Some(0xFD6B2546),
            accuraterip: AccurateRipTrack::Matched {
                confidence: 2,
                version: ArVersion::Unknown,
                crc: Some(0x32386D84),
            },
            ctdb: None,
        }
    );
    assert_eq!(log.ctdb, None);
    assert_eq!(matched_copies(&log), Some(2));
}

#[test]
fn eac_reads_the_ctdb_block_appended_after_the_status_report() {
    let log = log("eac-ctdb.log");

    assert_eq!(
        log.ripper,
        Ripper::Eac {
            version: "1.3".to_string()
        }
    );
    assert_eq!(log.tracks.len(), 9);
    assert_eq!(
        *track(&log, 8),
        TrackResult {
            number: 8,
            test_crc: Some(0xD5118F31),
            copy_crc: Some(0xD5118F31),
            accuraterip: AccurateRipTrack::Mismatch {
                confidence: Some(5),
                crc: Some(0xBF12B7A9),
                database_crc: Some(0x8E5D8011),
            },
            ctdb: Some(CtdbTrack::Differs {
                confidence: 16,
                total: 16,
                samples: 12,
            }),
        }
    );
    assert_eq!(
        log.ctdb,
        Some(CtdbSummary {
            tocid: "6CgGdovQOYDcrOIMxNcFY5wZilc-".to_string(),
            in_database: true,
        })
    );
    // Eight tracks match 5 AccurateRip copies and 16 CUETools copies, but
    // track 8 matches neither, so the release is not verified.
    assert_eq!(
        track(&log, 7).accuraterip,
        AccurateRipTrack::Matched {
            confidence: 5,
            version: ArVersion::V2,
            crc: Some(0x8D2FF96B),
        }
    );
    assert_eq!(matched_copies(&log), None);
}

#[test]
fn xld_reads_the_summary_and_the_signatures_under_each_track() {
    let log = log("xld-accuraterip-summary.log");

    assert_eq!(
        log.ripper,
        Ripper::Xld {
            version: "20170729 (150.3)".to_string()
        }
    );
    assert_eq!(log.tracks.len(), 11);
    assert_eq!(
        log.accuraterip,
        Some(AccurateRipSummary {
            disc_ids: Some((0x00111f68, 0x0093da86, 0x9209b40b)),
            in_database: true,
        })
    );
    assert_eq!(
        track(&log, 1).accuraterip,
        AccurateRipTrack::Matched {
            confidence: 7,
            version: ArVersion::V2,
            crc: Some(0x67D387BC),
        }
    );
    // `confidence 2+7/9` is two v1 copies plus seven v2 copies of the nine the
    // database holds.
    assert_eq!(
        track(&log, 2).accuraterip,
        AccurateRipTrack::Matched {
            confidence: 9,
            version: ArVersion::Both,
            crc: Some(0x93743C0E),
        }
    );
    // The one track whose copy CRC differs from its test CRC matched nothing.
    assert_eq!(
        *track(&log, 9),
        TrackResult {
            number: 9,
            test_crc: Some(0x606C50E6),
            copy_crc: Some(0x5076879F),
            accuraterip: AccurateRipTrack::Mismatch {
                confidence: None,
                crc: None,
                database_crc: None,
            },
            ctdb: None,
        }
    );
    assert_eq!(log.ctdb, None);
    assert_eq!(matched_copies(&log), None);
}

#[test]
fn xld_reads_a_disc_absent_from_accuraterip() {
    let log = log("xld-disc-not-found.log");

    assert_eq!(
        log.accuraterip,
        Some(AccurateRipSummary {
            disc_ids: None,
            in_database: false,
        })
    );
    assert_eq!(log.tracks.len(), 9);
    assert!(log
        .tracks
        .iter()
        .all(|track| track.accuraterip == AccurateRipTrack::NotInDatabase));
    assert_eq!(track(&log, 1).copy_crc, Some(0xB8DCFA71));
    assert_eq!(matched_copies(&log), None);
}

#[test]
fn older_xld_reads_one_unversioned_signature_per_track() {
    let log = log("xld-old.log");

    assert_eq!(
        log.ripper,
        Ripper::Xld {
            version: "20101010 (123.3)".to_string()
        }
    );
    assert_eq!(log.tracks.len(), 8);
    assert_eq!(
        *track(&log, 1),
        TrackResult {
            number: 1,
            test_crc: Some(0x92111A53),
            copy_crc: Some(0x92111A53),
            accuraterip: AccurateRipTrack::Matched {
                confidence: 1,
                version: ArVersion::Unknown,
                crc: Some(0xC4ADA5BB),
            },
            ctdb: None,
        }
    );
    // The log has no summary section at all — only the per-track verdicts.
    assert_eq!(log.accuraterip, None);
    assert_eq!(matched_copies(&log), Some(1));
}

#[test]
fn cueripper_reads_its_own_accuraterip_and_crc_tables() {
    let log = log("cueripper.log");

    assert_eq!(
        log.ripper,
        Ripper::CueRipper {
            version: "2.1.4".to_string()
        }
    );
    assert_eq!(log.tracks.len(), 4);
    assert_eq!(
        *track(&log, 1),
        TrackResult {
            number: 1,
            test_crc: None,
            copy_crc: Some(0xF57D3663),
            accuraterip: AccurateRipTrack::Matched {
                confidence: 9,
                version: ArVersion::Both,
                crc: Some(0x46837CE2),
            },
            ctdb: Some(CtdbTrack::Matched {
                confidence: 2,
                total: 2,
            }),
        }
    );
    assert_eq!(
        log.accuraterip,
        Some(AccurateRipSummary {
            disc_ids: Some((0x0007b198, 0x001eb52b, 0x2a0a6804)),
            in_database: true,
        })
    );
    // The tables that follow, re-checking the disc at other read offsets, name
    // the same tracks at lower counts and must not replace them.
    assert_eq!(
        track(&log, 4).accuraterip,
        AccurateRipTrack::Matched {
            confidence: 6,
            version: ArVersion::Both,
            crc: Some(0x5CA9A766),
        }
    );
    assert_eq!(matched_copies(&log), Some(6));
}

#[test]
fn a_log_with_no_banner_keeps_the_track_lines_it_can_read() {
    let log = parse_rip_log(concat!(
        "Track  1\n",
        "     Test CRC 0DDFDEF3\n",
        "     Copy CRC 0DDFDEF3\n",
        "     Accurately ripped (confidence 4)  [1D690F86]  (AR v1)\n",
    ))
    .expect("a log without a banner still carries results");

    assert_eq!(log.ripper, Ripper::Unknown);
    assert_eq!(
        track(&log, 1).accuraterip,
        AccurateRipTrack::Matched {
            confidence: 4,
            version: ArVersion::V1,
            crc: Some(0x1D690F86),
        }
    );
}

#[test]
fn eac_reads_an_absent_track_stated_without_xld_s_arrow() {
    let log = parse_rip_log(concat!(
        "Exact Audio Copy V1.6 from 23. October 2020\n",
        "\n",
        "Track  1\n",
        "\n",
        "     Test CRC 0DDFDEF3\n",
        "     Copy CRC 0DDFDEF3\n",
        "     Track not present in AccurateRip database\n",
        "     Copy OK\n",
    ))
    .expect("an EAC log should parse");

    assert_eq!(
        *track(&log, 1),
        TrackResult {
            number: 1,
            test_crc: Some(0x0DDFDEF3),
            copy_crc: Some(0x0DDFDEF3),
            accuraterip: AccurateRipTrack::NotInDatabase,
            ctdb: None,
        }
    );
    assert_eq!(matched_copies(&log), None);
}

#[test]
fn xld_reads_a_match_found_at_another_read_offset() {
    let log = parse_rip_log(concat!(
        "X Lossless Decoder version 20170729 (150.3)\n",
        "\n",
        "AccurateRip Summary (DiscID: 00111f68-0093da86-9209b40b)\n",
        "    Track 01 : OK (v1+v2, confidence 400/818, with different offset)\n",
    ))
    .expect("an XLD log should parse");

    // The offset is how it matched; the count is still how many agree.
    assert_eq!(
        track(&log, 1).accuraterip,
        AccurateRipTrack::Matched {
            confidence: 400,
            version: ArVersion::Both,
            crc: None,
        }
    );
    assert_eq!(matched_copies(&log), Some(400));
}

#[test]
fn a_disc_the_cuetools_database_does_not_hold_is_read_as_absent() {
    let log = parse_rip_log(concat!(
        "Exact Audio Copy V1.6 from 23. October 2020\n",
        "\n",
        "---- CUETools DB Plugin V2.1.6\n",
        "\n",
        "[CTDB TOCID: Vz63di0rg6yo7CGbc6_FtT59A9E-] disk not present in database\n",
    ))
    .expect("an EAC log should parse");

    assert_eq!(
        log.ctdb,
        Some(CtdbSummary {
            tocid: "Vz63di0rg6yo7CGbc6_FtT59A9E-".to_string(),
            in_database: false,
        })
    );
    assert!(log.tracks.is_empty());
}

#[test]
fn a_disc_accuraterip_does_not_hold_is_read_as_absent_with_its_id() {
    let log = parse_rip_log(concat!(
        "CUERipper v2.1.5 Copyright (C) 2008-13 Grigory Chudov\n",
        "\n",
        "AccurateRip summary\n",
        "\n",
        "[AccurateRip ID: 000b45b4-005aae3b-85075c0a] disk not present in database.\n",
    ))
    .expect("a CUERipper log should parse");

    assert_eq!(
        log.accuraterip,
        Some(AccurateRipSummary {
            disc_ids: Some((0x000b45b4, 0x005aae3b, 0x85075c0a)),
            in_database: false,
        })
    );
    assert!(log.tracks.is_empty());
}

#[test]
fn text_that_is_not_a_rip_log_is_rejected() {
    assert_eq!(
        parse_rip_log("Album Title\nrecorded 1998\n"),
        Err(RipLogError::NotARipLog)
    );
}

fn verification(tracks: &[(Option<u32>, Option<u32>)]) -> Verification {
    Verification {
        source: VerificationSource::Log,
        tracks: tracks
            .iter()
            .enumerate()
            .map(
                |(index, (accuraterip_confidence, ctdb_confidence))| TrackVerification {
                    number: index as u32 + 1,
                    accuraterip_confidence: *accuraterip_confidence,
                    ctdb_confidence: *ctdb_confidence,
                    crc: None,
                },
            )
            .collect(),
    }
}

#[test]
fn matched_copies_is_the_weakest_track_across_both_databases() {
    assert_eq!(
        verification(&[(Some(40), None), (Some(3), Some(17)), (None, Some(9))]).matched_copies(),
        Some(9)
    );
}

#[test]
fn one_track_no_database_confirms_leaves_the_release_unverified() {
    assert_eq!(
        verification(&[(Some(40), Some(40)), (None, None), (Some(40), Some(40))]).matched_copies(),
        None
    );
}

#[test]
fn a_release_with_no_tracks_is_unverified() {
    assert_eq!(verification(&[]).matched_copies(), None);
}
