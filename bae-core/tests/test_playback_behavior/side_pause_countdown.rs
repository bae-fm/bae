/// The countdown a side-pause state carries, if it is one.
fn side_pause_countdown(state: &PlaybackState) -> Option<PlaybackSideCountdown> {
    match state {
        PlaybackState::Paused {
            reason: PlaybackPauseReason::SideEnded(prompt),
            ..
        } => prompt.countdown.clone(),
        other => panic!("expected a side-ended pause, got {other:?}"),
    }
}

impl SidePauseTestFixture {
    /// Play side A's last track (A2) to its side pause and return that state.
    async fn pause_after_side_a(&mut self) -> PlaybackState {
        let side_a_track_id = self.track_ids[1].clone();
        self.play_to_side_pause(1, &side_a_track_id, "A", SIDE_PAUSE_TITLE_KEY)
            .await
    }

    /// Assert side B (B1) does not start within a short real-time window.
    async fn assert_next_side_waits(&mut self, message: &str) {
        let side_b_track_id = self.track_ids[2].clone();
        let started = self
            .wait_for_state(
                |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == side_b_track_id),
                Duration::from_millis(500),
            )
            .await;
        assert!(started.is_none(), "{message}");
    }

    async fn wait_for_next_side(&mut self, message: &str) {
        let side_b_track_id = self.track_ids[2].clone();
        self.wait_for_playing_track(&side_b_track_id, Duration::from_secs(5), message)
            .await;
    }

    /// Wait for the side pause to be announced again without a countdown.
    async fn wait_for_countdown_cancelled(&mut self) {
        self.wait_for_state(
            |s| {
                matches!(
                    s,
                    PlaybackState::Paused {
                        reason: PlaybackPauseReason::SideEnded(prompt),
                        ..
                    } if prompt.countdown.is_none()
                )
            },
            Duration::from_secs(5),
        )
        .await
        .expect("the side pause is announced without its countdown");
    }
}

#[tokio::test]
async fn side_pause_countdown_starts_the_next_side_when_it_runs_out() {
    let mut fixture = SidePauseTestFixture::with_countdown(SidePauseCountdown::Seconds5)
        .await
        .expect("side-pause fixture");

    let paused = fixture.pause_after_side_a().await;
    assert_eq!(
        side_pause_countdown(&paused),
        Some(PlaybackSideCountdown {
            resumes_at: side_pause_clock_start() + chrono::Duration::seconds(5),
            message_key: SIDE_PAUSE_COUNTDOWN_KEY,
        })
    );

    fixture.clock.advance(Duration::from_secs(4));
    fixture
        .assert_next_side_waits("a second before the countdown ends, side B waits")
        .await;

    fixture.clock.advance(Duration::from_secs(1));
    fixture
        .wait_for_next_side("side B starts when the countdown runs out")
        .await;
}

#[tokio::test]
async fn side_pause_without_a_countdown_waits_for_play() {
    let mut fixture = SidePauseTestFixture::with_countdown(SidePauseCountdown::Off)
        .await
        .expect("side-pause fixture");

    let paused = fixture.pause_after_side_a().await;
    assert_eq!(side_pause_countdown(&paused), None);

    fixture.clock.advance(Duration::from_secs(3600));
    fixture
        .assert_next_side_waits("with the countdown off, side B waits however long it takes")
        .await;

    fixture.playback_handle.resume();
    fixture.wait_for_next_side("Play starts side B").await;
}

#[tokio::test]
async fn closing_the_prompt_stops_the_countdown_and_keeps_the_pause() {
    let mut fixture = SidePauseTestFixture::with_countdown(SidePauseCountdown::Seconds15)
        .await
        .expect("side-pause fixture");
    fixture.pause_after_side_a().await;

    fixture.playback_handle.cancel_side_pause_countdown();
    fixture.wait_for_countdown_cancelled().await;

    fixture.clock.advance(Duration::from_secs(60));
    fixture
        .assert_next_side_waits("a closed prompt's countdown never starts side B")
        .await;

    fixture.playback_handle.resume();
    fixture
        .wait_for_next_side("Play still starts side B after the prompt was closed")
        .await;
}

#[tokio::test]
async fn play_during_the_countdown_starts_the_next_side_at_once() {
    let mut fixture = SidePauseTestFixture::with_countdown(SidePauseCountdown::Seconds60)
        .await
        .expect("side-pause fixture");
    fixture.pause_after_side_a().await;

    fixture.playback_handle.resume();
    fixture
        .wait_for_next_side("Play starts side B without waiting for the countdown")
        .await;
}

#[tokio::test]
async fn seeking_during_the_countdown_stops_it() {
    let mut fixture = SidePauseTestFixture::with_countdown(SidePauseCountdown::Seconds5)
        .await
        .expect("side-pause fixture");
    fixture.pause_after_side_a().await;

    fixture.playback_handle.seek(Duration::from_secs(1));
    fixture.wait_for_countdown_cancelled().await;

    fixture.clock.advance(Duration::from_secs(30));
    fixture
        .assert_next_side_waits("a seek stops the countdown")
        .await;
}

#[tokio::test]
async fn pausing_during_the_countdown_stops_it() {
    let mut fixture = SidePauseTestFixture::with_countdown(SidePauseCountdown::Seconds5)
        .await
        .expect("side-pause fixture");
    fixture.pause_after_side_a().await;

    fixture.playback_handle.pause();
    fixture.wait_for_countdown_cancelled().await;

    fixture.clock.advance(Duration::from_secs(30));
    fixture
        .assert_next_side_waits("a pause stops the countdown")
        .await;
}

#[tokio::test]
async fn a_queue_edit_during_the_countdown_stops_it() {
    let mut fixture = SidePauseTestFixture::with_countdown(SidePauseCountdown::Seconds5)
        .await
        .expect("side-pause fixture");
    fixture.pause_after_side_a().await;

    fixture
        .playback_handle
        .add_to_queue(vec![fixture.track_ids[0].clone()]);
    fixture.wait_for_countdown_cancelled().await;

    fixture.clock.advance(Duration::from_secs(30));
    fixture
        .assert_next_side_waits("a queue edit stops the countdown")
        .await;
}
