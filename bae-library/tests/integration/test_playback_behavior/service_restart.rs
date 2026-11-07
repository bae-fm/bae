use std::sync::atomic::{AtomicBool, Ordering};

use bae_playback::{AudioError, AudioOutput, AudioOutputDevice, PlaybackClock, PlaybackSleep};

use crate::test_playback_behavior::fixtures::*;
use crate::test_playback_behavior::support::*;

/// An audio device whose first opening fails, as a device that isn't there
/// yet does.
struct FailsOnce {
    device: Box<dyn AudioOutputDevice>,
    failed: AtomicBool,
}

impl AudioOutputDevice for FailsOnce {
    fn open_output(&self) -> Result<Box<dyn AudioOutput>, AudioError> {
        if self.failed.swap(true, Ordering::SeqCst) {
            self.device.open_output()
        } else {
            Err(AudioError::DeviceUnavailable)
        }
    }
}

/// A playback service over what the library holds, whose first attempt at
/// the audio device fails.
async fn start_service_whose_device_fails_once(
    library: &TestLibrary,
) -> (
    bae_playback::PlaybackHandle,
    bae_playback::PlaybackProgressFeed,
    bae_playback::test_capture::CaptureStreamRx,
    AudioClock,
) {
    let (device, capture_rx, audio_clock) = bae_playback::test_capture::manual_capture_device();
    let device = FailsOnce { device, failed: AtomicBool::new(false) };
    let handle = library
        .start_playback_on(
            Box::new(device),
            bae_playback::test_capture::wall_playback_clock(),
            true,
        )
        .await;
    let progress_rx = handle.subscribe_progress();
    (handle, progress_rx, capture_rx, audio_clock)
}

/// The press after a service ended resumes the track the resume row holds, at
/// its position and at the level the person set. Sabotage: start the fresh
/// service with nothing taken up, and the resume finds no track.
#[tokio::test(flavor = "multi_thread")]
async fn the_press_after_an_end_plays_on_from_the_saved_position_and_level() {
    with_fixture(MultiWindowPlayback::new("restart-plays-on"), async |playback| {
        let last_track = playback.track_ids[2].clone();
        playback.play_and_wait(&last_track).await;
        playback.playback_handle.seek(TrackTime::from_duration(Duration::from_secs(20)));
        wait_for_seeked_on(&mut playback.progress_rx).await.expect("the seek lands");
        playback.playback_handle.set_volume(0.3);
        bae_util::test_channels::next_matching(&mut playback.progress_rx, HANG_GUARD, |event| {
            matches!(event, PlaybackProgress::VolumeChanged { volume } if volume == 0.3)
                .then_some(())
        })
        .await
        .expect("the volume lands");
        playback.playback_handle.shutdown().await;

        let (handle, mut progress_rx, _capture_rx, audio_clock) =
            start_service_whose_device_fails_once(&playback.library).await;

        handle.resume();
        wait_for_state_on(
            &mut progress_rx,
            |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == last_track),
        )
        .await
        .expect("the resume plays the track the resume row holds");
        wait_for_position_advance(&handle, &audio_clock, &mut progress_rx)
            .await
            .expect("the resumed track plays on");
        let position = handle.subscribe_positions().positions().library.expect("a position");
        assert!(position.position_ms >= 20_000, "{position:?}");
        assert_eq!(
            bae_playback::tables::playback_level(playback.library.store()).await.unwrap(),
            (bae_playback::Volume::new(0.3).unwrap(), false)
        );
        handle.shutdown().await;
    })
    .await;
}

/// A fresh service plays nothing until a press asks it to: a press that
/// doesn't leaves the resumed track paused. Sabotage: restore the track
/// playing, and the pause finds it Playing first.
#[tokio::test(flavor = "multi_thread")]
async fn a_fresh_service_plays_nothing_until_a_press_asks_it_to() {
    with_fixture(MultiWindowPlayback::new("restart-stays-paused"), async |playback| {
        let track = playback.track_ids[0].clone();
        playback.play_and_wait(&track).await;
        playback.playback_handle.shutdown().await;

        let (handle, mut progress_rx, _capture_rx, _audio_clock) =
            start_service_whose_device_fails_once(&playback.library).await;

        handle.set_volume(0.5);
        let states = collect_states_on(&mut progress_rx, |s| {
            matches!(s, PlaybackState::Paused { track: paused, .. } if paused.track_id == track)
        })
        .await;
        handle.commands_handled().await;
        assert!(
            !states.iter().any(|s| matches!(s, PlaybackState::Playing { .. })),
            "nothing plays: {states:?}"
        );
        assert!(
            matches!(states.last(), Some(PlaybackState::Paused { .. })),
            "the track is loaded paused: {states:?}"
        );
        handle.shutdown().await;
    })
    .await;
}

/// A clock whose wait panics in the service loop, past the boundary that
/// catches what a wake does: a bug that ends the service in mid-session.
struct ClockThatBreaks(bae_playback::PlaybackClockRef);

impl PlaybackClock for ClockThatBreaks {
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        self.0.now()
    }

    fn sleep_until(&self, _deadline: chrono::DateTime<chrono::Utc>) -> PlaybackSleep {
        Box::pin(async { panic!("the clock broke") })
    }
}

/// A service a panic ends in mid-session stops once, with an error, and a
/// fresh one serves the next play, which what the ended one reported leaves
/// playing. Sabotage: start no fresh service, and the play fails.
#[tokio::test(flavor = "multi_thread")]
async fn a_service_that_ends_in_mid_session_is_reported_once_and_the_next_play_plays() {
    with_fixture(
        async {
            SidePauseTestFixture::with_countdown_on(SidePauseCountdown::Seconds5, |clock| {
                Arc::new(ClockThatBreaks(clock))
            })
            .await
            .expect("side-pause fixture")
        },
        async |fixture| {
            let side_a_track_id = fixture.track_ids[1].clone();
            let first_track_id = fixture.track_ids[0].clone();
            fixture.play_to_side_pause(&side_a_track_id, "A", PlaybackPauseBoundary::Side).await;

            // The countdown's wait broke the loop: playback stopped with why.
            let mut failures = 0;
            bae_util::test_channels::next_matching(&mut fixture.progress_rx, HANG_GUARD, |event| {
                match event {
                    PlaybackProgress::PlaybackError { .. } => failures += 1,
                    PlaybackProgress::ServiceEnded => return Some(()),
                    _ => {}
                }
                None
            })
            .await
            .expect("the service's end is told");
            assert_eq!(failures, 1);

            fixture.play_track_and_wait(&first_track_id).await;

            let events = fixture.settled_events().await;
            assert!(
                !events.iter().any(|event| matches!(event, PlaybackProgress::PlaybackError { .. })),
                "the fresh service reports nothing of the ended one: {events:?}"
            );
            assert!(
                !states_in(&events).iter().any(|s| matches!(s, PlaybackState::Stopped)),
                "the play goes on: {events:?}"
            );
        },
    )
    .await;
}
