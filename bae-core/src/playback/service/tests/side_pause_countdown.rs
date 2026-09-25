/// A side-pause phase resuming into `next`, with `resumes_at` as its countdown.
fn side_paused_slot(resumes_at: Option<chrono::DateTime<chrono::Utc>>) -> PlaybackSlot {
    let buffer = create_sparse_buffer(1_024);
    active_slot(
        test_prepared_track("t", buffer),
        TrackPhase::Paused(PausePhase::SideEnded(SidePauseDecision {
            track_id: "next".to_string(),
            boundary: SideBoundary {
                id: "next:1:Vinyl".to_string(),
                title_key: SIDE_PAUSE_TITLE_KEY,
                countdown_key: SIDE_PAUSE_COUNTDOWN_KEY,
                side_label: "A".to_string(),
            },
            resumes_at,
        })),
    )
}

/// Every state the service emitted since the last drain.
fn drained_states(
    progress_rx: &mut tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) -> Vec<PlaybackState> {
    let mut states = Vec::new();
    while let Ok(progress) = progress_rx.try_recv() {
        if let PlaybackProgress::StateChanged { state } = progress {
            states.push(state);
        }
    }
    states
}

fn countdown_deadline() -> chrono::DateTime<chrono::Utc> {
    test_clock_start() + chrono::Duration::seconds(5)
}

#[test]
fn side_pause_prompt_carries_the_countdown_worded_for_its_medium() {
    let decision = SidePauseDecision {
        track_id: "next".to_string(),
        boundary: SideBoundary {
            id: "next:1:Cd".to_string(),
            title_key: DISC_PAUSE_TITLE_KEY,
            countdown_key: DISC_PAUSE_COUNTDOWN_KEY,
            side_label: "1".to_string(),
        },
        resumes_at: Some(countdown_deadline()),
    };

    assert_eq!(
        decision.prompt(),
        PlaybackSidePausePrompt {
            id: "next:1:Cd".to_string(),
            title_key: DISC_PAUSE_TITLE_KEY,
            side_label: "1".to_string(),
            countdown: Some(PlaybackSideCountdown {
                resumes_at: countdown_deadline(),
                message_key: DISC_PAUSE_COUNTDOWN_KEY,
            }),
        }
    );
    assert_eq!(
        SidePauseDecision {
            resumes_at: None,
            ..decision
        }
        .prompt()
        .countdown,
        None
    );
}

#[tokio::test]
async fn cancelling_a_running_countdown_keeps_the_pause_and_announces_it() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    service.slot = side_paused_slot(Some(countdown_deadline()));

    service.cancel_side_pause_countdown();

    assert_eq!(service.side_pause_countdown_deadline(), None);
    assert!(service.is_side_paused(), "the pause itself stays");
    let states = drained_states(&mut progress_rx);
    assert!(
        matches!(
            states.as_slice(),
            [PlaybackState::Paused {
                reason: PlaybackPauseReason::SideEnded(prompt),
                ..
            }] if prompt.countdown.is_none()
        ),
        "one side-pause state without a countdown, got {states:?}"
    );
}

#[tokio::test]
async fn cancelling_with_no_countdown_running_changes_nothing() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    service.slot = side_paused_slot(None);

    service.cancel_side_pause_countdown();

    assert!(service.is_side_paused());
    assert!(drained_states(&mut progress_rx).is_empty());
}

#[tokio::test]
async fn a_countdown_that_is_no_longer_running_does_not_resume() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    service.slot = side_paused_slot(None);

    service
        .side_pause_countdown_elapsed(countdown_deadline())
        .await;

    assert!(service.is_side_paused(), "a cancelled countdown leaves the pause");
    assert!(drained_states(&mut progress_rx).is_empty());
}

#[test]
fn commands_that_steer_playback_stop_the_countdown_and_the_rest_leave_it() {
    let (volume_tx, _) = oneshot::channel();
    let stops = [
        PlaybackCommand::Pause,
        PlaybackCommand::Stop,
        PlaybackCommand::Next,
        PlaybackCommand::Previous,
        PlaybackCommand::Seek(std::time::Duration::ZERO),
        PlaybackCommand::SeekByRatio(0.5),
        PlaybackCommand::Play("track".to_string()),
        PlaybackCommand::PlayLibraryShuffled,
        PlaybackCommand::AddToQueue(vec!["track".to_string()]),
        PlaybackCommand::ClearUpNext,
        PlaybackCommand::SetShuffle(true),
        PlaybackCommand::StopRemote,
    ];
    for command in &stops {
        assert!(
            cancels_side_pause_countdown(command),
            "{command:?} stops the countdown"
        );
    }
    let leaves = [
        PlaybackCommand::Resume,
        PlaybackCommand::SetVolume(0.5),
        PlaybackCommand::SetMuted(true),
        PlaybackCommand::SetRepeatMode(RepeatMode::Context),
        PlaybackCommand::GetVolume(volume_tx),
        PlaybackCommand::AutoAdvance {
            track_id: "track".to_string(),
        },
    ];
    for command in &leaves {
        assert!(
            !cancels_side_pause_countdown(command),
            "{command:?} leaves the countdown running"
        );
    }
}
