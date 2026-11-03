use super::*;
use crate::queue::tests::*;
use crate::tables::ReleaseStart;

/// A repeating shuffled lane loops a freshly shuffled order each pass. Twelve
/// tracks, so two passes in the same order is one chance in 479,001,600.
#[tokio::test]
async fn test_context_repeat_shuffled_loops_a_reshuffled_order() {
    let tracks: Vec<String> = (1..=12).map(|i| format!("t{i}")).collect();
    let track_refs: Vec<&str> = tracks.iter().map(String::as_str).collect();
    let mut f = release(&track_refs).await;
    f.play(ReleaseStart::Shuffled).await;
    f.queue.set_repeat_mode(RepeatMode::Context).await.unwrap();
    let first_pass = f.full_order().await;
    for _ in 0..first_pass.len() - 1 {
        f.next().await;
    }
    assert!(f.next().await.is_some(), "the pass loops");
    let second_pass = f.full_order().await;

    assert_eq!(sorted(first_pass.clone()), sorted(second_pass.clone()));
    assert_ne!(first_pass, second_pass, "each pass is shuffled anew");
    assert!(!f.value().await.has_previous, "the new pass starts at its first track");
}

/// A sequential lane under `Context` repeat wraps to its first row in the
/// same order, carrying the session's removals and reorders into the next
/// pass.
#[tokio::test]
async fn test_context_repeat_sequential_wrap_carries_edits_into_the_next_pass() {
    let mut f = release(&["t1", "t2", "t3", "t4"]).await;
    f.play_from("t1").await;
    let (t2, t3, t4) = (f.entry("t2").await, f.entry("t3").await, f.entry("t4").await);
    f.queue.remove(t2).await.unwrap();
    f.queue.reorder(t4, Some(t3)).await.unwrap();
    assert_eq!(f.context_order().await, names(&["t1", "t4", "t3"]));

    f.queue.set_repeat_mode(RepeatMode::Context).await.unwrap();
    assert_eq!(f.next().await, Some("t4".into()));
    assert_eq!(f.next().await, Some("t3".into()));
    assert_eq!(f.next().await, Some("t1".into()));
    assert_eq!(
        f.context_order().await,
        names(&["t1", "t4", "t3"]),
        "the wrap replays the edited lane, not the source"
    );
}

/// The same track queued twice in Up Next plays twice.
#[tokio::test]
async fn test_a_track_queued_twice_plays_twice() {
    let mut f = release_and_manual(&["t1"], &["dup"]).await;
    f.play_from("t1").await;
    f.add(&["dup", "dup"]).await;
    assert_eq!(f.next().await, Some("dup".into()));
    assert_eq!(f.next().await, Some("dup".into()));
    assert_eq!(f.next().await, None);
}

#[tokio::test]
async fn test_manual_drains_before_context() {
    let mut f = release_and_manual(&["t1", "t2"], &["m1"]).await;
    f.play_from("t1").await;
    f.add(&["m1"]).await;
    assert_eq!(f.next().await, Some("m1".into()));
    assert_eq!(f.next().await, Some("t2".into()));
    assert_eq!(f.next().await, None);
}

#[tokio::test]
async fn test_context_advances_in_order() {
    let mut f = release(&["t1", "t2", "t3"]).await;
    f.play_from("t1").await;
    assert_eq!(f.next().await, Some("t2".into()));
    assert_eq!(f.next().await, Some("t3".into()));
    assert_eq!(f.next().await, None);
}

#[tokio::test]
async fn test_context_repeat_loops_from_stored_order() {
    let mut f = release(&["t1", "t2"]).await;
    f.play_from("t1").await;
    f.queue.set_repeat_mode(RepeatMode::Context).await.unwrap();
    assert_eq!(f.next().await, Some("t2".into()));
    assert_eq!(f.next().await, Some("t1".into()));
    assert_eq!(f.next().await, Some("t2".into()));
}

#[tokio::test]
async fn test_repeat_track_pins_current() {
    let mut f = release(&["t1", "t2"]).await;
    f.play_from("t1").await;
    f.queue.set_repeat_mode(RepeatMode::Track).await.unwrap();
    assert!(matches!(
        f.queue.next_entry().await.unwrap(),
        NextEntry::RepeatCurrent(t) if t == f.id("t1")
    ));
}

#[tokio::test]
async fn test_previous_steps_back_multiple() {
    let mut f = release(&["t1", "t2", "t3"]).await;
    f.play_from("t1").await;
    f.next().await;
    f.next().await;
    assert_eq!(f.previous(1_000).await, Some("t2".into()));
    assert_eq!(f.previous(1_000).await, Some("t1".into()));
    assert_eq!(f.previous(1_000).await, None, "at the start, Previous restarts");
}

#[tokio::test]
async fn test_previous_past_3s_restarts() {
    let mut f = release(&["t1", "t2"]).await;
    f.play_from("t2").await;
    assert_eq!(f.previous(5_000).await, None);
}

/// The threshold counts from the track's start (INDEX 01): Back steps back
/// while the pregap's countdown plays and up to just under 3 s into the track.
#[tokio::test]
async fn test_previous_threshold_counts_from_track_start() {
    for elapsed_ms in [-2_500, 0, 2_999] {
        let mut f = release(&["t1", "t2"]).await;
        f.play_from("t2").await;
        assert_eq!(
            f.previous(elapsed_ms).await,
            Some("t1".into()),
            "Back at {elapsed_ms}ms should step back"
        );
    }
    let mut f = release(&["t1", "t2"]).await;
    f.play_from("t2").await;
    assert_eq!(f.previous(3_000).await, None);
}

#[tokio::test]
async fn test_front_peeks_without_consuming() {
    let mut f = release(&["a", "b"]).await;
    assert_eq!(f.queue.front().await.unwrap(), None);
    f.add(&["a", "b"]).await;
    assert_eq!(f.queue.front().await.unwrap(), Some(f.id("a")));
    assert_eq!(f.upcoming().await, names(&["a", "b"]), "front must not consume");
}

/// The only edge that yields a physical side pause: the next track of a
/// sequential context, with Up Next empty. Every other shape is `None`.
#[tokio::test]
async fn test_next_sequential_context_track_all_branches() {
    let mut f = release(&["t1", "t2", "t3", "m1"]).await;
    assert_eq!(f.queue.next_sequential_context_track().await.unwrap(), None);

    f.play_from("t1").await;
    assert_eq!(f.queue.next_sequential_context_track().await.unwrap(), Some(f.id("t2")));

    f.add(&["m1"]).await;
    assert_eq!(f.queue.next_sequential_context_track().await.unwrap(), None);

    f.play(ReleaseStart::Shuffled).await;
    assert_eq!(f.queue.next_sequential_context_track().await.unwrap(), None);

    f.play_from("m1").await;
    assert_eq!(
        f.queue.next_sequential_context_track().await.unwrap(),
        None,
        "the last track has nothing after it"
    );
}

/// Shuffling closes the physical-side-pause gate and unshuffling reopens it.
#[tokio::test]
async fn test_shuffle_closes_the_side_pause_gate_and_unshuffle_reopens_it() {
    let mut f = release(&["t1", "t2", "t3"]).await;
    f.play_from("t1").await;
    assert_eq!(f.queue.next_sequential_context_track().await.unwrap(), Some(f.id("t2")));
    f.queue.set_shuffle(true).await.unwrap();
    assert_eq!(f.queue.next_sequential_context_track().await.unwrap(), None);
    f.queue.set_shuffle(false).await.unwrap();
    assert_eq!(f.queue.next_sequential_context_track().await.unwrap(), Some(f.id("t2")));
}

/// When the current track is an Up Next entry, stepping back within 3s lands
/// on the context track it followed.
#[tokio::test]
async fn test_previous_action_from_manual_current_lands_on_cursor() {
    let mut f = release_and_manual(&["t1", "t2", "t3"], &["m1"]).await;
    f.play_from("t1").await;
    f.add(&["m1"]).await;
    assert_eq!(f.next().await, Some("m1".into()));
    assert_eq!(f.previous(1_000).await, Some("t1".into()));
}
