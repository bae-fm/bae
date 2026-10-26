/// A side-pause phase resuming into `next`, with `resumes_at` as its countdown.
fn side_paused_slot(resumes_at: Option<chrono::DateTime<chrono::Utc>>) -> PlaybackSlot {
    let buffer = create_sparse_buffer(1_024);
    active_slot(
        test_prepared_track("t", buffer),
        TrackPhase::Paused(PausePhase::SideEnded(SidePauseDecision {
            track_id: "next".to_string(),
            boundary: SideBoundary {
                id: "next:1:Vinyl".to_string(),
                kind: PlaybackPauseBoundary::Side,
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

/// Two tracks of one release, the first on `last` = (medium, side) and the
/// second on `next`, each side ending `boundary` and labelled as given.
fn tracks_across(
    boundary: PlaybackPauseBoundary,
    last: (u16, u8, &str),
    next: (u16, u8, &str),
) -> (PlaybackTrackInfo, PlaybackTrackInfo) {
    let on = |track_id: &str, (medium, side, label): (u16, u8, &str)| PlaybackTrackInfo {
        side: Some(PlaybackTrackSide {
            medium: std::num::NonZeroU16::new(medium).expect("mediums count from one"),
            side: std::num::NonZeroU8::new(side).expect("sides count from one"),
            boundary,
            label: label.to_string(),
        }),
        ..test_track_info(track_id)
    };
    (on("last", last), on("next", next))
}

/// A record's side ends at the next side of it, and a CD at the next disc;
/// the prompt carries which kind ended and the label of the side that did.
#[test]
fn a_pause_carries_the_kind_of_boundary_that_ended() {
    for (kind, last, next, side_label) in [
        (PlaybackPauseBoundary::Side, (1, 2, "B"), (2, 1, "C"), "B"),
        (PlaybackPauseBoundary::Side, (1, 1, "A"), (1, 2, "B"), "A"),
        (PlaybackPauseBoundary::Disc, (2, 1, "2"), (3, 1, "3"), "2"),
    ] {
        let (last, next) = tracks_across(kind, last, next);
        let boundary = side_boundary_between(&last, &next).expect("the side ends");
        assert_eq!(boundary.kind, kind, "{side_label}");
        assert_eq!(boundary.side_label, side_label);
        let prompt = SidePauseDecision {
            track_id: "next".to_string(),
            boundary,
            resumes_at: None,
        }
        .prompt();
        assert_eq!(prompt.boundary, kind, "{side_label}");
    }
}

#[test]
fn side_pause_prompt_carries_its_countdown() {
    let decision = SidePauseDecision {
        track_id: "next".to_string(),
        boundary: SideBoundary {
            id: "next:1:Cd".to_string(),
            kind: PlaybackPauseBoundary::Disc,
            side_label: "1".to_string(),
        },
        resumes_at: Some(countdown_deadline()),
    };

    assert_eq!(
        decision.prompt(),
        PlaybackSidePausePrompt {
            id: "next:1:Cd".to_string(),
            boundary: PlaybackPauseBoundary::Disc,
            side_label: "1".to_string(),
            countdown: Some(PlaybackSideCountdown {
                resumes_at: countdown_deadline(),
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
        PlaybackCommand::Seek(TrackTime::START),
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

/// What playback reads of a track on a release of `mediums`, its place given
/// as (medium, side).
fn info_on(
    track_id: &str,
    mediums: &crate::db::DbReleaseMediums,
    (medium, side): (u16, u8),
) -> PlaybackTrackInfo {
    let mut track = crate::db::DbTrack::new_test("release-id", track_id, "Track Title", 1);
    track.medium_id.clone_from(&mediums.0[usize::from(medium) - 1].id);
    track.side = std::num::NonZeroU8::new(side).expect("sides count from one");
    PlaybackTrackInfo::of(&track, mediums)
}

fn release_of(mediums: &[crate::mediums::ReleaseMedium]) -> crate::db::DbReleaseMediums {
    crate::db::DbReleaseMediums(
        mediums
            .iter()
            .enumerate()
            .map(|(index, medium)| {
                crate::db::DbReleaseMedium::new(
                    "release-id",
                    crate::mediums::medium_position(index),
                    *medium,
                    chrono::Utc::now(),
                )
            })
            .collect(),
    )
}

/// A CD release pauses where its discs change and nowhere else, the pause
/// named by the disc that ended.
#[test]
fn a_cd_release_pauses_at_disc_changes_only() {
    let cd = crate::mediums::ReleaseMedium::one_sided(Some(bae_types::pressing::Medium::Cd));
    let mediums = release_of(&[cd, cd]);
    let first = info_on("first", &mediums, (1, 1));
    let second = info_on("second", &mediums, (1, 1));
    let third = info_on("third", &mediums, (2, 1));
    assert_eq!(side_boundary_between(&first, &second), None);
    let boundary = side_boundary_between(&second, &third).expect("disc 1 ends");
    assert_eq!(boundary.kind, PlaybackPauseBoundary::Disc);
    assert_eq!(boundary.side_label, "1");
}

/// A 2×LP rip pauses at each side, named by its letter across both records.
#[test]
fn a_two_record_release_pauses_at_each_side() {
    let record = crate::mediums::ReleaseMedium {
        carrier: Some(bae_types::pressing::Medium::Vinyl),
        sides: std::num::NonZeroU8::new(2).expect("two sides"),
    };
    let mediums = release_of(&[record, record]);
    let places = [(1, 1), (1, 2), (2, 1), (2, 2)];
    let infos: Vec<_> = places
        .iter()
        .enumerate()
        .map(|(index, place)| info_on(&format!("track-{index}"), &mediums, *place))
        .collect();
    let labels: Vec<_> = infos
        .windows(2)
        .map(|pair| {
            let boundary = side_boundary_between(&pair[0], &pair[1]).expect("a side ends");
            assert_eq!(boundary.kind, PlaybackPauseBoundary::Side);
            boundary.side_label
        })
        .collect();
    assert_eq!(labels, ["A", "B", "C"]);
}

/// A medium of no physical carrier never pauses.
#[test]
fn a_medium_of_no_carrier_never_pauses() {
    let unstated = crate::mediums::ReleaseMedium::one_sided(None);
    let mediums = release_of(&[unstated, unstated]);
    let first = info_on("first", &mediums, (1, 1));
    let second = info_on("second", &mediums, (2, 1));
    assert_eq!(first.side, None);
    assert_eq!(side_boundary_between(&first, &second), None);
}
