use super::*;
use crate::audio_output::AudioEvent;
use crate::service::tests::*;

fn starved_event(
    track_id: &str,
    starved_ms: u64,
    samples_decoded: u64,
    producer_finished: bool,
) -> AudioEvent {
    AudioEvent::Starved {
        fmt: Arc::new(TrackFmt::for_test(track_id)),
        starved_ms,
        position: StreamPosition::START,
        producer_finished,
        samples_decoded,
        decode_errors: 0,
        has_next: false,
    }
}

fn starvation_ended_event(track_id: &str) -> AudioEvent {
    AudioEvent::StarvationEnded {
        fmt: Arc::new(TrackFmt::for_test(track_id)),
        starved_ms: 0,
        position: StreamPosition::START,
        samples_decoded: 0,
        decode_errors: 0,
    }
}

/// The starvation telemetry says where in the track playback starved, as the
/// player shows it: in a 2 s pregap, 1.5 s into the stream is 0.5 s before the
/// track's start.
#[tokio::test]
async fn starvation_onset_reports_the_track_time_it_starved_at() {
    let (diagnostics, transport) = recording_diagnostics();
    let store = seeded_store(&[]).await;
    let (mut service, _progress_rx) = playback_service_over(&store, diagnostics.clone());
    let mut prepared = test_prepared_track("t", create_sparse_buffer(1_024));
    prepared.timeline = TrackTimeline::new(
        Some(std::time::Duration::from_secs(10)),
        std::time::Duration::from_millis(2_000),
    );
    let fmt = prepared.track_fmt(StreamPosition::START);
    service.slot = active_slot(prepared, TrackPhase::Playing);

    service
        .handle_audio_event(AudioEvent::Starved {
            fmt: Arc::new(fmt),
            starved_ms: 500,
            position: StreamPosition::from_millis(1_500),
            producer_finished: false,
            samples_decoded: 1_000,
            decode_errors: 0,
            has_next: false,
        })
        .await;

    diagnostics.flush().await.expect("flush succeeds");
    let starved: Vec<serde_json::Value> = transport
        .requests()
        .iter()
        .flat_map(|request| {
            serde_json::from_slice::<Vec<bae_diagnostics::DiagnosticEvent>>(&request.body)
                .expect("recorded request body is a diagnostic-event array")
        })
        .filter(|event| event.name == "playback_starved")
        .map(|event| event.fields["track_time"].clone())
        .collect();
    assert_eq!(starved, [serde_json::json!(-500)]);
}

/// Starvation with no decode progress past the fail threshold is a real stall,
/// so it surfaces a `PlaybackError` and stops playback instead of freezing.
#[tokio::test]
async fn starvation_past_fail_threshold_with_no_progress_escalates_to_error_and_stops() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service.handle_audio_event(starved_event("t", 500, 1_000, false)).await;
    service.handle_audio_event(starved_event("t", 30_000, 1_000, false)).await;

    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PlaybackError { .. }) {
            saw_error = true;
        }
    }
    assert!(saw_error, "a stalled starvation episode must surface a PlaybackError");
    assert!(matches!(service.slot, PlaybackSlot::Stopped), "the stalled track must stop");
}

/// `samples_decoded` advancing between `Starved` events shows the producer is
/// alive (a slow cloud fetch, say), so starvation never escalates however long
/// it lasts.
#[tokio::test]
async fn starvation_with_advancing_samples_decoded_never_escalates() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service.handle_audio_event(starved_event("t", 500, 1_000, false)).await;
    service.handle_audio_event(starved_event("t", 30_000, 1_100, false)).await;
    service.handle_audio_event(starved_event("t", 60_000, 1_200, false)).await;

    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PlaybackError { .. }) {
            saw_error = true;
        }
    }
    assert!(!saw_error, "advancing samples_decoded must never escalate, regardless of starved_ms");
    assert!(matches!(service.slot, PlaybackSlot::Active(_)), "the track must stay active");
}

/// `producer_finished` means a drained track awaiting `AutoAdvance`, not a stall.
#[tokio::test]
async fn starvation_with_producer_finished_never_escalates() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service.handle_audio_event(starved_event("t", 500, 1_000, true)).await;
    service.handle_audio_event(starved_event("t", 60_000, 1_000, true)).await;

    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PlaybackError { .. }) {
            saw_error = true;
        }
    }
    assert!(!saw_error, "producer_finished starvation is the completion path, never escalates");
}

/// A `StarvationEnded` resets the watchdog, so the next episode starts fresh.
/// Without the reset, the second `Starved` below, already past the threshold,
/// would continue the first episode and escalate.
#[tokio::test]
async fn starvation_ended_resets_the_episode_clock() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    // The first episode ends before reaching the fail threshold.
    service.handle_audio_event(starved_event("t", 500, 1_000, false)).await;
    service.handle_audio_event(starvation_ended_event("t")).await;

    // A second episode begins at the same samples_decoded count.
    service.handle_audio_event(starved_event("t", 30_000, 1_000, false)).await;

    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PlaybackError { .. }) {
            saw_error = true;
        }
    }
    assert!(
        !saw_error,
        "StarvationEnded must reset the episode; the first Starved event after \
         it establishes a fresh baseline rather than escalating immediately"
    );
    assert!(matches!(service.slot, PlaybackSlot::Active(_)), "the track must stay active");
}
