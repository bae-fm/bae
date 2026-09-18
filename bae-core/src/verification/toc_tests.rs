use super::super::test_discs::{enhanced_cd, from_fixture};
use super::*;

#[test]
fn reads_the_toc_table_a_rip_log_prints() {
    let toc = from_fixture("test_album.log");

    assert_eq!(toc.tracks().len(), 10);
    assert_eq!(toc.audio_track_count(), 10);
    assert_eq!(toc.first_audio_track().start, 0);
    assert_eq!(toc.tracks()[1].start, 23410);
    assert_eq!(toc.last_audio_track().end, 188814);
    assert_eq!(toc.leadout(), 188815);
    assert!(toc.tracks().iter().all(|track| track.audio));
    assert_eq!(toc.tracks()[0].end, 23409);
}

#[test]
fn a_text_with_no_toc_table_is_not_a_disc() {
    let error = DiscToc::from_log_text("no table of contents here\n")
        .expect_err("a log with no TOC cannot describe a disc");

    assert!(
        matches!(error, VerificationError::InvalidToc { .. }),
        "unexpected error: {error}"
    );
}

#[test]
fn a_disc_with_no_audio_has_nothing_to_verify() {
    let data_only = vec![TocTrack {
        number: 1,
        start: 0,
        end: 999,
        audio: false,
    }];

    assert!(DiscToc::new(data_only, 1000).is_err());
}

#[test]
fn tracks_may_not_overlap_or_run_past_the_lead_out() {
    let overlapping = vec![
        TocTrack {
            number: 1,
            start: 0,
            end: 100,
            audio: true,
        },
        TocTrack {
            number: 2,
            start: 100,
            end: 200,
            audio: true,
        },
    ];
    assert!(DiscToc::new(overlapping, 201).is_err());

    let past_leadout = vec![TocTrack {
        number: 1,
        start: 0,
        end: 100,
        audio: true,
    }];
    assert!(DiscToc::new(past_leadout, 100).is_err());
}

#[test]
fn an_enhanced_disc_keeps_its_data_track_out_of_the_audio_count() {
    let toc = enhanced_cd();

    assert_eq!(toc.tracks().len(), 11);
    assert_eq!(toc.audio_track_count(), 10);
    // The audio session ends 11400 sectors before the data track begins, so the
    // last audio track's end is not the next track's start.
    assert_eq!(toc.last_audio_track().end, 270_184);
    assert_eq!(
        toc.tracks()[10].start - (toc.last_audio_track().end + 1),
        11_400
    );
    assert_eq!(toc.leadout(), 333651);
}

/// Write a cue sheet and read it back the way the import pass does.
fn sheet(content: &str) -> (tempfile::TempDir, crate::cue_flac::CueSheet) {
    let dir = tempfile::tempdir().expect("a temp dir for the sheet");
    let path = dir.path().join("disc.cue");
    std::fs::write(&path, content).expect("writing a cue fixture");
    let sheet = crate::cue_flac::parse_cue_sheet(&path).expect("cue fixture should parse");
    (dir, sheet)
}

#[test]
fn reads_the_disc_a_cue_sheet_and_its_audio_lay_out() {
    let (_dir, parsed) = sheet(
        "FILE \"01.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n\
         FILE \"02.flac\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n",
    );
    // 5000 ms is 375 sectors; 3000 ms is 225.
    let audio = [
        crate::import::discid::SheetAudioDuration {
            file_reference: "01.flac",
            duration_ms: 5_000,
        },
        crate::import::discid::SheetAudioDuration {
            file_reference: "02.flac",
            duration_ms: 3_000,
        },
    ];

    let toc = DiscToc::from_cue(&parsed, &audio).expect("an all-audio sheet lays out a disc");

    assert_eq!(
        toc.tracks(),
        [
            TocTrack {
                number: 1,
                start: 0,
                end: 374,
                audio: true
            },
            TocTrack {
                number: 2,
                start: 375,
                end: 599,
                audio: true
            },
        ]
    );
    assert_eq!(toc.leadout(), 600);
}

#[test]
fn a_cue_sheet_that_declares_a_data_track_lays_out_no_disc() {
    let (_dir, parsed) = sheet(
        "FILE \"image.wav\" WAVE\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n\
         \n  TRACK 02 AUDIO\n    INDEX 01 00:05:00\n",
    );
    let audio = [crate::import::discid::SheetAudioDuration {
        file_reference: "image.wav",
        duration_ms: 10_000,
    }];

    let error = DiscToc::from_cue(&parsed, &audio)
        .expect_err("a data track's sectors are in no audio file");

    assert!(
        matches!(error, VerificationError::InvalidToc { .. }),
        "unexpected error: {error}"
    );
}
