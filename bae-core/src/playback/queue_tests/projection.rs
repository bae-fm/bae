#[tokio::test]
async fn test_front_peeks_without_consuming() {
    let mut f = release(&["a", "b"]).await;
    assert_eq!(f.queue.front().await.unwrap(), None);
    f.add(&["a", "b"]).await;
    assert_eq!(f.queue.front().await.unwrap(), Some(f.id("a")));
    assert_eq!(f.upcoming().await, names(&["a", "b"]), "front must not consume");
}

/// The value keeps the two lanes separate: Up Next is its own list, the
/// context is its not-yet-played tail.
#[tokio::test]
async fn test_value_keeps_manual_and_context_separate() {
    let mut f = release_and_manual(&["t1", "t2", "t3"], &["m1", "m2"]).await;
    f.play_from("t1").await;
    f.add(&["m1", "m2"]).await;

    let value = f.value().await;
    let manual: Vec<String> = value.manual.iter().map(|i| f.name(&i.track_id)).collect();
    assert_eq!(manual, names(&["m1", "m2"]));
    let context = value.context.expect("a release is playing");
    assert_eq!(
        f.db.queue_upcoming().await.unwrap().iter().map(|i| f.name(&i.track_id)).collect::<Vec<_>>(),
        names(&["t2", "t3"])
    );
    assert!(!context.shuffled);
    assert_eq!(context.source, ContextSource::Release(f.id("r1")));
    assert_eq!(context.source_title.as_deref(), Some("Album Title"));
}

/// No context → no context section; Up Next still shows on its own.
#[tokio::test]
async fn test_value_with_no_context() {
    let mut f = release(&["m1", "m2"]).await;
    f.add(&["m1", "m2"]).await;
    let value = f.value().await;
    assert!(value.context.is_none());
    assert_eq!(value.manual.len(), 2);
    assert!(value.has_next);
    assert!(!value.has_previous);
}

#[tokio::test]
async fn test_has_next_and_has_previous() {
    let mut f = release(&["t1", "t2"]).await;
    let empty = f.value().await;
    assert!(!empty.has_next && !empty.has_previous);
    f.play_from("t1").await;
    let first = f.value().await;
    assert!(first.has_next && !first.has_previous);
    f.next().await;
    let last = f.value().await;
    assert!(!last.has_next && last.has_previous);
    f.queue.set_repeat_mode(RepeatMode::Context).await.unwrap();
    assert!(f.value().await.has_next, "repeat always has a next track");
}

/// The context lane's tracks are read in windows by offset from the playing
/// track, each read with the tail's length.
#[tokio::test]
async fn test_offset_windows_read_with_the_tail_length() {
    let tracks: Vec<String> = (0..130).map(|i| format!("t{i:03}")).collect();
    let refs: Vec<&str> = tracks.iter().map(String::as_str).collect();
    let mut f = release(&refs).await;
    f.play_from("t000").await;

    let windows = crate::library::LibraryPageWindows::from([
        crate::library::LibraryPageWindow { offset: 0, limit: 3 },
        crate::library::LibraryPageWindow { offset: 100, limit: 20 },
        crate::library::LibraryPageWindow { offset: 125, limit: 20 },
    ]);
    let mut live = f.db.subscribe_queue_upcoming(windows);
    let read = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    let window_tracks: Vec<Vec<String>> = read
        .windows
        .iter()
        .map(|w| w.items.iter().map(|i| f.name(&i.track_id)).collect())
        .collect();
    assert_eq!(window_tracks[0], tracks[1..4].to_vec());
    assert_eq!(window_tracks[1], tracks[101..121].to_vec());
    assert_eq!(window_tracks[2], tracks[126..130].to_vec(), "a window past the end holds what remains");
    assert_eq!(read.upcoming_total, 129);
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

/// With nothing playing there is no context to reorder: no context appears
/// and no track becomes current.
#[tokio::test]
async fn test_set_shuffle_with_no_context_is_noop() {
    let mut f = release(&["t1"]).await;
    f.queue.set_shuffle(true).await.unwrap();
    assert!(f.value().await.context.is_none());
    assert_eq!(f.current_name(), None);
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

/// Reordering a context entry with `before = None` moves it to the end of the
/// lane while the playing track stays playing.
#[tokio::test]
async fn test_reorder_context_to_end() {
    let mut f = release(&["t1", "t2", "t3", "t4"]).await;
    f.play_from("t1").await;
    let t2 = f.entry("t2").await;
    f.queue.reorder(t2, None).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["t3", "t4", "t2"]));
    assert_eq!(f.current(), "t1");
}
