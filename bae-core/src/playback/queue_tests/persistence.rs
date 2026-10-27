/// The queue lives in its tables, so a queue taken up again over them — a
/// restart — has the same lanes in the same order, the same playing track
/// and the same history: a shuffled lane keeps the order it was shuffled in.
#[tokio::test]
async fn test_restore_takes_up_the_stored_order() {
    let mut f = release_and_manual(&["t1", "t2", "t3", "t4", "t5"], &["m1"]).await;
    f.play(ReleaseStart::Shuffled).await;
    f.next().await;
    f.next().await;
    f.add(&["m1"]).await;
    f.queue.set_repeat_mode(RepeatMode::Context).await.unwrap();
    let current = f.queue.current().cloned();
    let lane = f.context_order().await;
    let value = f.value().await;

    let mut restored = PlaybackQueue::new(f.db.playback_queue_tables());
    restored.restore(current.clone()).await.unwrap();
    f.queue = restored;

    assert_eq!(f.queue.current().cloned(), current);
    assert_eq!(f.queue.repeat_mode(), RepeatMode::Context);
    assert_eq!(f.context_order().await, lane, "the shuffled order survives");
    assert_eq!(f.value().await, value);
    assert!(f.value().await.has_previous, "the history survives");
    assert_eq!(f.previous(1_000).await.as_deref(), Some(lane[1].as_str()));
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
async fn test_skip_to_context_tail_moves_cursor() {
    let mut f = release(&["t1", "t2", "t3", "t4"]).await;
    f.play_from("t1").await;
    let t3 = f.entry("t3").await;
    let entry = f.queue.skip_to(t3).await.unwrap();
    assert_eq!(entry.map(|e| f.name(&e.track_id)), Some("t3".into()));
    assert_eq!(f.current(), "t3");
    assert_eq!(f.upcoming().await, names(&["t4"]));
}

#[tokio::test]
async fn test_remove_context_tail_entry() {
    let mut f = release(&["t1", "t2", "t3"]).await;
    f.play_from("t1").await;
    let t2 = f.entry("t2").await;
    let removed = f.queue.remove(t2).await.unwrap();
    assert_eq!(removed.map(|e| f.name(&e.track_id)), Some("t2".into()));
    assert_eq!(f.upcoming().await, names(&["t3"]));
}

#[tokio::test]
async fn test_reorder_context_tail_keeps_cursor() {
    let mut f = release(&["t1", "t2", "t3", "t4"]).await;
    f.play_from("t1").await;
    let (t2, t3) = (f.entry("t2").await, f.entry("t3").await);
    f.queue.reorder(t3, Some(t2)).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["t3", "t2", "t4"]));
    assert_eq!(f.current(), "t1", "the playing track stays playing");
}

/// Moving the playing context row carries it: what follows it afterwards is
/// what follows its new place.
#[tokio::test]
async fn test_reorder_of_the_playing_context_row_moves_the_cursor_with_it() {
    let mut f = release(&["t1", "t2", "t3", "t4"]).await;
    f.play_from("t2").await;
    let playing = f.queue.current().unwrap().id.clone();
    let t4 = f.entry("t4").await;
    f.queue.reorder(playing, Some(t4)).await.unwrap();
    assert_eq!(f.context_order().await, names(&["t1", "t3", "t2", "t4"]));
    assert_eq!(f.upcoming().await, names(&["t4"]));
    assert_eq!(f.previous(1_000).await, Some("t3".into()));
}

#[tokio::test]
async fn test_reorder_cross_lane_is_noop() {
    let mut f = release_and_manual(&["t1", "t2"], &["m1"]).await;
    f.play_from("t1").await;
    f.add(&["m1"]).await;
    let manual = f.manual_ids().await[0].clone();
    let context = f.entry("t2").await;
    assert!(!f.queue.reorder(manual, Some(context)).await.unwrap());
    assert_eq!(f.upcoming().await, names(&["m1", "t2"]));
}

#[tokio::test]
async fn test_insert_at_middle() {
    let mut f = release(&["a", "b", "c", "x", "y"]).await;
    f.add(&["a", "b", "c"]).await;
    f.queue.insert_at(1, f.ids(&["x", "y"])).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["a", "x", "y", "b", "c"]));
}

#[tokio::test]
async fn test_insert_at_beyond_end_clamps() {
    let mut f = release(&["a", "x"]).await;
    f.add(&["a"]).await;
    f.queue.insert_at(999, f.ids(&["x"])).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["a", "x"]));
}

/// Removing the playing context row clears the current entry, and the row
/// after it is still the one that plays next: no track is passed over.
#[tokio::test]
async fn test_removing_the_playing_context_row_keeps_the_next_one_upcoming() {
    let mut f = release(&["t1", "t2", "t3"]).await;
    f.play_from("t2").await;
    let playing = f.queue.current().unwrap().id.clone();

    f.queue.remove(playing).await.unwrap();

    assert_eq!(f.current_name(), None, "removing the playing row clears current");
    assert_eq!(f.upcoming().await, names(&["t3"]), "t3 still plays next");
    assert_eq!(f.next().await, Some("t3".into()));
    assert_eq!(f.previous(1_000).await, Some("t1".into()));
}

/// The library deleting the playing track forgets it as current; the queue's
/// rows of it left with the track.
#[tokio::test]
async fn test_forget_deleted_current() {
    let mut f = release(&["t1", "t2"]).await;
    f.play_from("t1").await;
    let other: std::collections::HashSet<String> = [f.id("t2")].into();
    assert!(!f.queue.forget_deleted_current(&other));
    assert_eq!(f.current(), "t1");
    let playing: std::collections::HashSet<String> = [f.id("t1")].into();
    assert!(f.queue.forget_deleted_current(&playing));
    assert_eq!(f.current_name(), None);
}
