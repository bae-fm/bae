//! How many files playback holds open: those of the playing track and of the
//! one preloaded after it, which a track reading two files makes four.

use crate::test_playback_behavior::support::*;
use bae_audio::EncodeFormat;

const SAMPLE_RATE: u32 = 44_100;
const FILE_SECONDS: u32 = 3;
const FILES: [&str; 3] = ["01 Track One.wav", "02 Track Two.wav", "03 Track Three.wav"];

fn tone(frequency: f64) -> Vec<i32> {
    let frames = (SAMPLE_RATE * FILE_SECONDS) as usize;
    (0..frames * 2)
        .map(|i| {
            let t = (i / 2) as f64 / f64::from(SAMPLE_RATE);
            let v = 0.1 * (2.0 * std::f64::consts::PI * frequency * t).sin();
            ((v * f64::from(i16::MAX)) as i32) << 16
        })
        .collect()
}

/// A gap-appended rip: three files, the second and third tracks each starting
/// their pregap in the file before their own, so each of them reads two files.
fn write_gap_appended_rip(dir: &std::path::Path) {
    for (name, frequency) in FILES.iter().zip([220.0, 440.0, 880.0]) {
        let audio = bae_audio::encode_i32(
            EncodeFormat::PcmWav { bits_per_sample: 16 },
            &tone(frequency),
            SAMPLE_RATE,
            2,
        )
        .expect("encode a file");
        std::fs::write(dir.join(name), audio).unwrap();
    }
    std::fs::write(
        dir.join("Album Title.cue"),
        format!(
            "FILE \"{}\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Track One\"\n    INDEX 01 00:00:00\n  \
             TRACK 02 AUDIO\n    TITLE \"Track Two\"\n    INDEX 00 00:02:00\n\
             FILE \"{}\" WAVE\n    INDEX 01 00:00:00\n  \
             TRACK 03 AUDIO\n    TITLE \"Track Three\"\n    INDEX 00 00:02:00\n\
             FILE \"{}\" WAVE\n    INDEX 01 00:00:00\n",
            FILES[0], FILES[1], FILES[2]
        ),
    )
    .unwrap();
}

/// Playback over tracks that each read two files, within a budget of exactly
/// four places, never holds more than four files open and never waits for a
/// place: it plays, advances, skips forward and back, jumps and stops, each
/// step finishing inside the test's time limit.
#[tokio::test(flavor = "multi_thread")]
async fn playback_opens_at_most_four_files_across_advances() {
    bae_util::test_logs::install();
    bae_audio::init();
    let budget = coven::FileBudget::new(std::num::NonZeroUsize::new(4).unwrap());
    let watch = budget.watch();
    let release = bae_catalogs::discogs::models::DiscogsRelease {
        master_id: Some("gap-appended-master".to_string()),
        ..bae_catalogs::test_releases::discogs_test_release(
            "gap-appended-playback",
            "Album Title",
            &[("Track One", "0:02"), ("Track Two", "0:02"), ("Track Three", "0:03")],
        )
    };
    let (library, imported) = bae_library::test_libraries::imported_release_setup_within(
        &bae_catalogs::fake_catalogs::FakeCatalogs::offline(),
        budget,
        release,
        "test",
        coven::IdSource::new_id(&coven::UuidIds).to_string(),
        write_gap_appended_rip,
    )
    .await
    .expect("import the rip within four places");
    let [one, two, three] = <[String; 3]>::try_from(imported.track_ids.clone()).unwrap();
    let (handle, _capture, _clock) =
        bae_library::test_libraries::start_manual_playback(&library).await;
    let mut progress_rx = handle.subscribe_progress();
    let playing = |track: &str| {
        let track = track.to_string();
        move |state: &PlaybackState| matches!(state, PlaybackState::Playing { track: playing, .. } if playing.track_id == track)
    };

    play_and_wait_on(&handle, &mut progress_rx, &one).await;
    handle.next();
    wait_for_state_on(&mut progress_rx, playing(&two)).await.expect("track two plays");
    handle.next();
    wait_for_state_on(&mut progress_rx, playing(&three)).await.expect("track three plays");
    handle.previous();
    wait_for_state_on(&mut progress_rx, playing(&two)).await.expect("track two plays again");
    play_and_wait_on(&handle, &mut progress_rx, &three).await;
    play_and_wait_on(&handle, &mut progress_rx, &one).await;
    handle.stop();
    wait_for_state_on(&mut progress_rx, |state| matches!(state, PlaybackState::Stopped))
        .await
        .expect("playback stops");

    assert!(
        watch.most_holding() <= 4,
        "playback held {} files open at once, over its four places",
        watch.most_holding()
    );
    // Every fill ends and gives its file and place back once playback stops.
    tokio::time::timeout(HANG_GUARD, async {
        while watch.holding() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("every place is back after playback stops");
    library.manager().close().await.expect("close the test library");
}
