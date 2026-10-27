/// A system that grants the speaker or refuses it, counting what it was asked.
struct CountingAudioFocus {
    refuses: bool,
    requests: std::sync::atomic::AtomicUsize,
    releases: std::sync::atomic::AtomicUsize,
}

impl CountingAudioFocus {
    fn new(refuses: bool) -> Arc<Self> {
        Arc::new(Self {
            refuses,
            requests: std::sync::atomic::AtomicUsize::new(0),
            releases: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    fn requests(&self) -> usize {
        self.requests.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn releases(&self) -> usize {
        self.releases.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl crate::playback::AudioFocusSystem for CountingAudioFocus {
    fn request(&self) -> Result<(), crate::playback::AudioFocusRefused> {
        self.requests
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.refuses {
            Err(crate::playback::AudioFocusRefused(
                "a call holds the speaker".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    fn release(&self) {
        self.releases
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// A service whose platform shares the speaker through `system`.
async fn service_sharing_the_speaker(
    system: Arc<CountingAudioFocus>,
) -> (
    TempDir,
    PlaybackService,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    let (home, mut service, progress_rx) =
        seeded_playback_service(&[(FOCUS_RELEASE, &[TRACK_T, FOCUS_NEXT])]).await;
    service.audio_focus = crate::playback::AudioFocus::Shared(system);
    (home, service, progress_rx)
}

/// A track paused by the person, with the callback silent.
fn manually_paused(service: &mut PlaybackService) {
    service.slot = active_slot(
        test_prepared_track(TRACK_T, create_sparse_buffer(1_024)),
        TrackPhase::Paused(PausePhase::Manual),
    );
    service
        .audio_output
        .set_state(crate::playback::audio_output::AudioState::Paused);
}

/// The release these tests play, and its second track.
const FOCUS_RELEASE: &str = "5f0fb2d4-0a39-4f43-9b0c-6a8f1b7f3d21";
const FOCUS_NEXT: &str = "8c2e7f14-61d3-4b5a-a0de-2b6c9e4f7a13";

/// The queue `t` then `next`, with `next` decoded ahead and ready to play.
async fn next_preloaded(service: &mut PlaybackService) {
    fill_test_release(service, FOCUS_RELEASE).await;
    let (_sink, next_source, _ready) = create_track_stream_pair(44_100, 2);
    service.preloaded_next = Some(test_preloaded_next(
        test_prepared_track(FOCUS_NEXT, create_sparse_buffer(1_024)),
        PreloadedNextSource::Held(next_source),
    ));
}

fn current_phase_is_paused(service: &PlaybackService, track_id: &str) -> bool {
    matches!(
        &service.slot,
        PlaybackSlot::Active(cur)
            if cur.prepared.track_id == track_id
                && matches!(cur.phase, TrackPhase::Paused(PausePhase::Manual))
    )
}

#[tokio::test]
async fn play_while_the_system_refuses_the_speaker_stays_paused_and_shows_it() {
    let system = CountingAudioFocus::new(true);
    let (_home, mut service, mut progress_rx) = service_sharing_the_speaker(system.clone()).await;
    manually_paused(&mut service);

    service.resume().await;

    assert_eq!(system.requests(), 1, "the system is asked before playing");
    assert!(current_phase_is_paused(&service, TRACK_T));
    assert_eq!(
        service.audio_output.get_state(),
        crate::playback::audio_output::AudioState::Paused,
        "no sound plays"
    );
    let states = drained_states(&mut progress_rx);
    assert!(
        matches!(states.as_slice(), [PlaybackState::Paused { .. }]),
        "every UI is told it stays paused, got {states:?}"
    );
}

#[tokio::test]
async fn play_while_the_system_grants_the_speaker_plays() {
    let system = CountingAudioFocus::new(false);
    let (_home, mut service, _progress_rx) = service_sharing_the_speaker(system.clone()).await;
    manually_paused(&mut service);

    service.resume().await;

    assert_eq!(system.requests(), 1);
    assert_eq!(
        service.audio_output.get_state(),
        crate::playback::audio_output::AudioState::Playing
    );
}

#[tokio::test]
async fn next_from_a_side_pause_the_system_refuses_loads_the_next_side_paused() {
    let system = CountingAudioFocus::new(true);
    let (_home, mut service, _progress_rx) = service_sharing_the_speaker(system.clone()).await;
    service.slot = side_paused_between(TRACK_T, FOCUS_NEXT, None);
    next_preloaded(&mut service).await;

    service.handle_next().await;

    assert_eq!(system.requests(), 1);
    assert!(
        current_phase_is_paused(&service, FOCUS_NEXT),
        "the next side is current and paused"
    );
    assert_eq!(
        service.audio_output.get_state(),
        crate::playback::audio_output::AudioState::Paused
    );
}

#[tokio::test]
async fn a_countdown_the_system_refuses_loads_the_next_side_paused_and_stops_counting() {
    let system = CountingAudioFocus::new(true);
    let (_home, mut service, _progress_rx) = service_sharing_the_speaker(system.clone()).await;
    service.slot = side_paused_between(TRACK_T, FOCUS_NEXT, Some(countdown_deadline()));
    next_preloaded(&mut service).await;

    service
        .side_pause_countdown_elapsed(countdown_deadline())
        .await;

    assert_eq!(system.requests(), 1);
    assert!(current_phase_is_paused(&service, FOCUS_NEXT));
    assert_eq!(
        service.side_pause_countdown_deadline(),
        None,
        "nothing is left counting down to try again"
    );
}

#[tokio::test]
async fn a_start_that_lands_paused_does_not_ask_for_the_speaker() {
    let system = CountingAudioFocus::new(true);
    let (_home, mut service, _progress_rx) = service_sharing_the_speaker(system.clone()).await;
    manually_paused(&mut service);
    next_preloaded(&mut service).await;

    service.handle_next().await;

    assert_eq!(system.requests(), 0, "a paused Next makes no sound");
    assert!(current_phase_is_paused(&service, FOCUS_NEXT));
}

#[tokio::test]
async fn stopping_gives_the_speaker_back() {
    let system = CountingAudioFocus::new(false);
    let (_home, mut service, _progress_rx) = service_sharing_the_speaker(system.clone()).await;
    manually_paused(&mut service);

    service.stop().await;

    assert_eq!(system.releases(), 1);
}
