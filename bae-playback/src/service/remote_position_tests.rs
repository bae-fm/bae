//! A device's position and its play in history between its statuses: worked
//! out from the last status, its rate and when it arrived.

use std::time::Duration;

use super::*;
use crate::renderer::{ReceiverStatus, RendererPlayerState, RendererSessionEvent};
use crate::service::tests::*;
use crate::PlaybackClock;

const INTERVAL: Duration = Duration::from_millis(50);

fn playing_at(seconds: u64, rate: Option<f64>) -> ReceiverStatus {
    ReceiverStatus {
        player_state: RendererPlayerState::Playing,
        position: Some(Duration::from_secs(seconds)),
        duration: None,
        volume: Some(1.0),
        rate,
    }
}

/// A service playing remotely over a fake device, on a clock only the test
/// moves.
async fn on_a_manual_clock() -> (
    SeededStore,
    PlaybackService,
    Arc<crate::ManualPlaybackClock>,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    let (home, mut service, _device, rx) = remote_over_fake().await;
    let clock = Arc::new(crate::ManualPlaybackClock::new(test_clock_start()));
    service.clock = clock.clone();
    (home, service, clock, rx)
}

async fn hear(service: &mut PlaybackService, status: ReceiverStatus) {
    let observed_at = service.clock.now();
    let session = super::remote_session_tests::live_session(service);
    service
        .handle_remote_event(RendererSessionEvent::Status { session, status, observed_at })
        .await;
}

/// The position updates `rx` holds, in track time.
fn updates(rx: &mut tokio_mpsc::UnboundedReceiver<PlaybackProgress>) -> Vec<u64> {
    let mut shown = Vec::new();
    while let Ok(progress) = rx.try_recv() {
        if let PlaybackProgress::PositionUpdate { position_ms, .. } = progress {
            shown.push(position_ms.try_into().expect("a position is not negative"));
        }
    }
    shown
}

#[tokio::test]
async fn the_shown_position_moves_on_each_tick_with_no_new_status() {
    let (_home, mut service, clock, mut rx) = on_a_manual_clock().await;
    hear(&mut service, playing_at(30, Some(1.0))).await;
    updates(&mut rx);

    let first = service.remote_display_deadline().expect("a tick is due while the device plays");
    assert_eq!(first, test_clock_start() + chrono::Duration::from_std(INTERVAL).unwrap());
    clock.advance(INTERVAL);
    service.remote_display_tick(first);
    assert_eq!(updates(&mut rx), [30_050]);

    let second = service.remote_display_deadline().expect("the next tick");
    assert_eq!(second, first + chrono::Duration::from_std(INTERVAL).unwrap());
    clock.advance(Duration::from_millis(950));
    service.remote_display_tick(second);
    assert_eq!(updates(&mut rx), [31_000]);
    assert!(service.remote_display_deadline().is_some_and(|next| next > clock.now()));
}

#[tokio::test]
async fn nothing_ticks_while_the_device_is_paused_or_states_no_rate() {
    let (_home, mut service, _clock, _rx) = on_a_manual_clock().await;
    hear(&mut service, playing_at(30, None)).await;
    assert_eq!(service.remote_display_deadline(), None, "an unreadable rate holds the position");

    hear(
        &mut service,
        ReceiverStatus { player_state: RendererPlayerState::Paused, ..playing_at(30, Some(1.0)) },
    )
    .await;
    assert_eq!(service.remote_display_deadline(), None);
}

#[tokio::test]
async fn a_seek_waits_for_the_devices_next_status_before_the_position_moves_on() {
    let (_home, mut service, _clock, _rx) = on_a_manual_clock().await;
    hear(&mut service, playing_at(30, Some(1.0))).await;
    assert!(service.remote_display_deadline().is_some());
    service.seek(StreamPosition::from_millis(90_000)).await;
    assert_eq!(
        service.remote_display_deadline(),
        None,
        "the old status would pull the position back to before the seek"
    );
}

/// How long after `start` the play is due to count as played, from the
/// service's deadline.
fn due_after(service: &PlaybackService, start: chrono::DateTime<chrono::Utc>) -> Duration {
    (service.audible_deadline().expect("the play is due to count") - start)
        .to_std()
        .expect("after the start")
}

#[tokio::test]
async fn a_play_counts_when_the_clock_reaches_its_deadline_and_not_before() {
    let (_home, mut service, clock, _rx) = on_a_manual_clock().await;
    hear(&mut service, playing_at(0, Some(1.0))).await;
    let start = clock.now();
    let wait = due_after(&service, start);
    assert!(wait > Duration::ZERO);
    assert!(service.people.history().await.unwrap().is_empty());

    clock.advance(wait - Duration::from_millis(1));
    service.observe_remote_now();
    service.checkpoint_listening().await;
    assert!(service.people.history().await.unwrap().is_empty(), "a millisecond early");
    assert_eq!(service.audible_deadline(), Some(start + chrono::Duration::from_std(wait).unwrap()));

    clock.advance(Duration::from_millis(1));
    service.observe_remote_now();
    service.checkpoint_listening().await;
    assert_eq!(service.people.history().await.unwrap().len(), 1, "at the deadline");
    assert_eq!(service.audible_deadline(), None);
}

#[tokio::test]
async fn at_half_speed_a_play_takes_twice_as_long_to_count() {
    let (_home, mut service, clock, _rx) = on_a_manual_clock().await;
    hear(&mut service, playing_at(0, Some(1.0))).await;
    let full = due_after(&service, clock.now());

    hear(&mut service, playing_at(0, Some(0.5))).await;
    assert_eq!(due_after(&service, clock.now()), full * 2);
}

#[tokio::test]
async fn a_play_that_starts_in_the_pregap_counts_once_the_track_body_has_played() {
    let (_home, mut service, clock, _rx) = on_a_manual_clock().await;
    hear(&mut service, playing_at(0, Some(1.0))).await;
    let without_pregap = due_after(&service, clock.now());

    if let PlaybackSlot::Active(cur) = &mut service.slot {
        cur.prepared.timeline =
            TrackTimeline::new(cur.prepared.timeline.duration(), Duration::from_secs(6));
    }
    hear(&mut service, playing_at(4, Some(1.0))).await;
    assert_eq!(
        due_after(&service, clock.now()),
        Duration::from_secs(2) + without_pregap,
        "two seconds to the body, then what a play needs"
    );
}

#[tokio::test]
async fn stopping_between_statuses_saves_what_was_heard_up_to_the_stop() {
    let (_home, mut service, clock, _rx) = on_a_manual_clock().await;
    hear(&mut service, playing_at(0, Some(1.0))).await;
    clock.advance(Duration::from_secs(40));

    service.stop().await;

    let rows = service.people.history().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].played, Duration::from_secs(40));
}
