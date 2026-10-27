#[tokio::test]
async fn test_add_to_queue_fills_manual_lane() {
    let mut f = release(&["a", "b", "c"]).await;
    f.add(&["a", "b", "c"]).await;
    assert_eq!(f.upcoming().await, names(&["a", "b", "c"]));
}

#[tokio::test]
async fn test_add_to_queue_gives_duplicates_distinct_ids() {
    let mut f = release(&["a"]).await;
    f.add(&["a", "a"]).await;
    let ids = f.manual_ids().await;
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1], "duplicate tracks get distinct ids");
}

/// A track the library does not hold is left out, and the count says so.
#[tokio::test]
async fn test_add_to_queue_leaves_out_tracks_the_library_lacks() {
    let mut f = release(&["a"]).await;
    let added = f
        .queue
        .add_to_queue(vec![f.id("a"), uuid::Uuid::new_v4().to_string()])
        .await
        .unwrap();
    assert_eq!(added, 1);
    assert_eq!(f.upcoming().await, names(&["a"]));
}

#[tokio::test]
async fn test_add_next_preserves_order() {
    let mut f = release(&["a", "b", "x"]).await;
    f.add(&["x"]).await;
    f.queue.add_next(f.ids(&["a", "b"])).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["a", "b", "x"]));
}

#[tokio::test]
async fn test_remove_by_entry_id() {
    let mut f = release(&["a", "b", "c"]).await;
    f.add(&["a", "b", "c"]).await;
    let b_id = f.manual_ids().await[1];
    let removed = f.queue.remove(b_id).await.unwrap();
    assert_eq!(removed.map(|e| f.name(&e.track_id)), Some("b".into()));
    assert_eq!(f.upcoming().await, names(&["a", "c"]));
}

/// The same track enqueued twice, removing one instance by its id leaves the
/// other instance — and its id — intact.
#[tokio::test]
async fn test_remove_one_duplicate_keeps_the_other() {
    let mut f = release(&["dup"]).await;
    f.add(&["dup", "dup"]).await;
    let ids = f.manual_ids().await;
    let removed = f.queue.remove(ids[0]).await.unwrap().expect("first instance removed");
    assert_eq!(removed.id, ids[0]);
    assert_eq!(f.manual_ids().await, vec![ids[1]], "the other instance's id survives");
}

#[tokio::test]
async fn test_remove_unknown_id_is_noop() {
    let mut f = release(&["a"]).await;
    f.add(&["a"]).await;
    let before = f.shown().await;
    assert_eq!(f.queue.remove(QueueEntryId(i64::MAX)).await.unwrap(), None);
    assert_eq!(f.upcoming().await, names(&["a"]));
    assert_eq!(f.shown().await, before, "an unknown id changes nothing");
}

#[tokio::test]
async fn test_reorder_forward() {
    let mut f = release(&["a", "b", "c", "d"]).await;
    f.add(&["a", "b", "c", "d"]).await;
    let ids = f.manual_ids().await;
    assert!(f.queue.reorder(ids[0], Some(ids[2])).await.unwrap());
    assert_eq!(f.upcoming().await, names(&["b", "a", "c", "d"]));
}

#[tokio::test]
async fn test_reorder_to_end() {
    let mut f = release(&["a", "b", "c", "d"]).await;
    f.add(&["a", "b", "c", "d"]).await;
    let ids = f.manual_ids().await;
    f.queue.reorder(ids[0], None).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["b", "c", "d", "a"]));
}

#[tokio::test]
async fn test_reorder_backward() {
    let mut f = release(&["a", "b", "c", "d"]).await;
    f.add(&["a", "b", "c", "d"]).await;
    let ids = f.manual_ids().await;
    f.queue.reorder(ids[2], Some(ids[0])).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["c", "a", "b", "d"]));
}

/// A reorder over a lane with gaps — entries removed from between — keeps
/// every survivor's place.
#[tokio::test]
async fn test_reorder_across_gaps() {
    let mut f = release(&["a", "b", "c", "d", "e"]).await;
    f.add(&["a", "b", "c", "d", "e"]).await;
    let ids = f.manual_ids().await;
    f.queue.remove(ids[1]).await.unwrap();
    f.queue.remove(ids[3]).await.unwrap();
    f.queue.reorder(ids[0], Some(ids[4])).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["c", "a", "e"]));
    f.queue.reorder(ids[4], Some(ids[2])).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["e", "c", "a"]));
}

#[tokio::test]
async fn test_reorder_before_self_is_noop() {
    let mut f = release(&["a", "b", "c"]).await;
    f.add(&["a", "b", "c"]).await;
    let ids = f.manual_ids().await;
    f.queue.reorder(ids[1], Some(ids[1])).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["a", "b", "c"]));
}

#[tokio::test]
async fn test_reorder_unknown_source_is_noop() {
    let mut f = release(&["a", "b"]).await;
    f.add(&["a", "b"]).await;
    assert!(!f.queue.reorder(QueueEntryId(i64::MAX), None).await.unwrap());
    assert_eq!(f.upcoming().await, names(&["a", "b"]));
}

#[tokio::test]
async fn test_clear_up_next_empties_manual_keeps_context() {
    let mut f = release_and_manual(&["t1", "t2", "t3"], &["m1", "m2"]).await;
    f.play_from("t1").await;
    f.add(&["m1", "m2"]).await;
    f.queue.clear_up_next().await.unwrap();
    assert_eq!(f.upcoming().await, names(&["t2", "t3"]));
}

/// Clearing the context lane drops its rows, its history, and its label
/// while the playing track keeps playing — the lane it came from is gone,
/// not the audio.
#[tokio::test]
async fn test_clear_playing_from_drops_the_context_keeping_the_current_track() {
    let mut f = release(&["t1", "t2", "t3"]).await;
    f.play_from("t2").await;
    assert!(f.value().await.has_previous, "t1 sits behind the cursor");

    f.queue.clear_playing_from().await.unwrap();

    assert_eq!(f.current(), "t2", "the playing track keeps playing");
    let value = f.value().await;
    assert!(value.context.is_none(), "the context section is gone");
    assert!(!value.has_previous, "its history went with it");
    assert!(f.upcoming().await.is_empty());
}

/// After clearing the context lane, Up Next drains and then playback stops —
/// there is no lane left to fall through to.
#[tokio::test]
async fn test_clear_playing_from_then_up_next_drains_and_stops() {
    let mut f = release_and_manual(&["t1", "t2", "t3"], &["m1"]).await;
    f.play_from("t1").await;
    f.add(&["m1"]).await;

    f.queue.clear_playing_from().await.unwrap();

    assert_eq!(f.next().await, Some("m1".into()));
    assert_eq!(f.next().await, None);
}

#[tokio::test]
async fn test_clear_playing_from_with_no_context_is_noop() {
    let mut f = release(&["m1"]).await;
    f.add(&["m1"]).await;
    let before = f.shown().await;

    f.queue.clear_playing_from().await.unwrap();

    assert_eq!(f.shown().await, before, "nothing to clear");
    assert_eq!(f.upcoming().await, names(&["m1"]));
}

#[tokio::test]
async fn test_skip_to_manual_drains_prefix() {
    let mut f = release(&["a", "b", "c", "d"]).await;
    f.add(&["a", "b", "c", "d"]).await;
    let c_id = f.manual_ids().await[2];
    let entry = f.queue.skip_to(c_id).await.unwrap();
    assert_eq!(entry.map(|e| f.name(&e.track_id)), Some("c".into()));
    assert_eq!(f.upcoming().await, names(&["d"]));
    assert_eq!(f.current(), "c");
}

#[tokio::test]
async fn test_skip_to_unknown_id_is_noop() {
    let mut f = release(&["a"]).await;
    f.add(&["a"]).await;
    assert_eq!(f.queue.skip_to(QueueEntryId(i64::MAX)).await.unwrap(), None);
    assert_eq!(f.upcoming().await, names(&["a"]));
}

// -- context ---------------------------------------------------------------

#[tokio::test]
async fn test_play_release_sets_current_and_upcoming() {
    let mut f = release(&["t1", "t2", "t3"]).await;
    assert_eq!(f.play(ReleaseStart::First).await, "t1");
    assert_eq!(f.current(), "t1");
    assert_eq!(f.upcoming().await, names(&["t2", "t3"]));
}

#[tokio::test]
async fn test_play_release_from_a_track() {
    let mut f = release(&["t1", "t2", "t3"]).await;
    assert_eq!(f.play_from("t2").await, "t2");
    assert_eq!(f.upcoming().await, names(&["t3"]));
}

/// A track the release does not hold, and a release with no tracks, play
/// nothing and leave the queue as it was.
#[tokio::test]
async fn test_play_that_names_nothing_leaves_the_queue() {
    let mut f = fixture(&[("r1", &["t1", "t2"]), ("r2", &["other"])]).await;
    f.play_from("t1").await;
    let before = f.value().await;

    let missing = f
        .queue
        .play(ContextFill::Release {
            release_id: f.id("r1"),
            start: ReleaseStart::Track(f.id("other")),
        })
        .await
        .unwrap();
    assert_eq!(missing, crate::db::ContextFilled::StartMissing);
    let empty = f
        .queue
        .play(ContextFill::Release {
            release_id: uuid::Uuid::new_v4().to_string(),
            start: ReleaseStart::First,
        })
        .await
        .unwrap();
    assert_eq!(empty, crate::db::ContextFilled::NoTracks);

    assert_eq!(f.value().await, before);
    assert_eq!(f.current(), "t1");
}

/// Starting playback from a release replaces the active playback context, so
/// manually queued entries from the previous context do not carry into it.
#[tokio::test]
async fn test_play_release_clears_up_next() {
    let mut f = release_and_manual(&["t1", "t2", "t3"], &["m1", "m2"]).await;
    f.add(&["m1", "m2"]).await;

    assert_eq!(f.play(ReleaseStart::First).await, "t1");

    assert_eq!(
        f.upcoming().await,
        names(&["t2", "t3"]),
        "the new context replaces the old Up Next lane"
    );
    assert_eq!(f.next().await, Some("t2".into()));
}

#[tokio::test]
async fn test_play_release_shuffled_keeps_all_tracks() {
    let mut f = release(&["t1", "t2", "t3", "t4"]).await;
    f.play(ReleaseStart::Shuffled).await;
    assert_eq!(sorted(f.full_order().await), names(&["t1", "t2", "t3", "t4"]));
    assert!(f.value().await.context.unwrap().shuffled);
}

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

/// Unshuffling after a repeat wrap puts every upcoming row back in the order
/// the lane had when shuffle turned on.
#[tokio::test]
async fn test_unshuffle_after_a_repeat_wrap_lands_in_the_stamped_order() {
    let mut f = release(&["t1", "t2", "t3", "t4", "t5"]).await;
    f.play_from("t1").await;
    f.queue.set_shuffle(true).await.unwrap();
    f.queue.set_repeat_mode(RepeatMode::Context).await.unwrap();
    for _ in 0..4 {
        f.next().await;
    }
    assert!(f.next().await.is_some());

    f.queue.set_shuffle(false).await.unwrap();

    let wrapped_current = f.current();
    let expected: Vec<String> = names(&["t1", "t2", "t3", "t4", "t5"])
        .into_iter()
        .filter(|t| *t != wrapped_current)
        .collect();
    let upcoming = f.upcoming().await;
    let behind = f.context_order().await.len() - upcoming.len() - 1;
    assert_eq!(
        upcoming,
        expected[behind..].to_vec(),
        "every row after the wrap's playing one sits in the stamped order"
    );
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

/// A `Library` play holds every library track, shuffled, and its source is
/// the library; unshuffling puts it in the library's own order.
#[tokio::test]
async fn test_library_play_holds_all_tracks_in_library_order() {
    let mut f = fixture(&[("r1", &["a1", "a2"]), ("r2", &["b1", "b2", "b3"])]).await;
    let first = f.play_fill(ContextFill::LibraryShuffled).await;
    assert_eq!(sorted(f.full_order().await), names(&["a1", "a2", "b1", "b2", "b3"]));
    let value = f.value().await;
    let context = value.context.unwrap();
    assert_eq!(context.source, ContextSource::Library);
    assert!(context.shuffled);

    f.queue.set_shuffle(false).await.unwrap();

    // The library's order is by release, then position.
    let mut releases = [("r1", vec!["a1", "a2"]), ("r2", vec!["b1", "b2", "b3"])];
    releases.sort_by_key(|(release, _)| f.id(release));
    let library_order: Vec<String> = releases
        .iter()
        .flat_map(|(_, tracks)| tracks.iter().map(|t| t.to_string()))
        .filter(|t| *t != first)
        .collect();
    assert_eq!(f.upcoming().await, library_order);
}

// -- shuffle toggle --------------------------------------------------------

/// Turning shuffle on is surgery on the upcoming tail alone: the current row
/// and the history before it stay where they were, and every unplayed row
/// stays upcoming. Run several times, since a shuffle that happens to leave a
/// row in place would hide the bug.
#[tokio::test]
async fn test_set_shuffle_on_keeps_history_and_shuffles_only_upcoming() {
    for _ in 0..8 {
        let mut f = release(&["t1", "t2", "t3", "t4", "t5"]).await;
        f.play_from("t3").await;
        f.queue.set_shuffle(true).await.unwrap();

        assert_eq!(f.current(), "t3");
        assert!(f.value().await.context.unwrap().shuffled);
        assert_eq!(
            f.context_order().await[..3],
            names(&["t1", "t2", "t3"])[..],
            "the history and the current row never move"
        );
        assert_eq!(sorted(f.upcoming().await), names(&["t4", "t5"]));
    }
}

/// The lane is the authority: a reorder made before shuffling round-trips
/// exactly, because the order shuffle returns to is the lane's — not the
/// album's.
#[tokio::test]
async fn test_shuffle_round_trip_restores_a_reordered_lane_not_source_order() {
    let mut f = release(&["t1", "t2", "t3", "t4", "t5"]).await;
    f.play_from("t1").await;
    let (t5, t2) = (f.entry("t5").await, f.entry("t2").await);
    f.queue.reorder(t5, Some(t2)).await.unwrap();
    let reordered = f.upcoming().await;
    assert_eq!(reordered, names(&["t5", "t2", "t3", "t4"]));

    f.queue.set_shuffle(true).await.unwrap();
    f.queue.set_shuffle(false).await.unwrap();

    assert_eq!(f.upcoming().await, reordered);
}

/// Shuffle on then off with no edits in between round-trips to the original
/// lane order.
#[tokio::test]
async fn test_shuffle_round_trip_with_no_edits_is_the_identity() {
    let mut f = release(&["t1", "t2", "t3", "t4", "t5", "t6"]).await;
    f.play_from("t2").await;
    let before = f.context_order().await;

    f.queue.set_shuffle(true).await.unwrap();
    f.queue.set_shuffle(false).await.unwrap();

    assert_eq!(f.context_order().await, before);
    assert_eq!(f.current(), "t2");
}

/// A row removed while shuffled stays gone after unshuffling, and every other
/// unplayed row keeps its place in line.
#[tokio::test]
async fn test_a_row_removed_while_shuffled_is_absent_after_unshuffling() {
    let mut f = release(&["t1", "t2", "t3", "t4", "t5"]).await;
    f.play_from("t1").await;
    f.queue.set_shuffle(true).await.unwrap();
    let removed = f.upcoming_entries().await[0].clone();
    f.queue.remove(removed.id).await.unwrap();

    f.queue.set_shuffle(false).await.unwrap();

    let removed_track = f.name(&removed.track_id);
    assert_eq!(f.current(), "t1");
    let expected: Vec<String> = names(&["t2", "t3", "t4", "t5"])
        .into_iter()
        .filter(|t| *t != removed_track)
        .collect();
    assert_eq!(f.upcoming().await, expected);
}

/// Asking for the order the lane already has changes nothing.
#[tokio::test]
async fn test_set_shuffle_to_the_current_state_is_idempotent() {
    let mut f = release(&["t1", "t2", "t3", "t4", "t5"]).await;
    f.play_from("t1").await;
    f.queue.set_shuffle(true).await.unwrap();
    let order = f.context_order().await;
    let before = f.shown().await;

    f.queue.set_shuffle(true).await.unwrap();

    assert_eq!(f.context_order().await, order, "the lane is not shuffled again");
    assert_eq!(f.shown().await, before);

    f.queue.set_shuffle(false).await.unwrap();
    let sequential = f.shown().await;
    f.queue.set_shuffle(false).await.unwrap();
    assert_eq!(f.shown().await, sequential);
}

#[tokio::test]
async fn test_set_shuffle_off_restores_source_order_from_current() {
    let mut f = release(&["t1", "t2", "t3", "t4", "t5"]).await;
    f.play_from("t3").await;
    f.queue.set_shuffle(true).await.unwrap();
    assert_eq!(f.current(), "t3");

    f.queue.set_shuffle(false).await.unwrap();

    assert_eq!(f.current(), "t3");
    assert!(!f.value().await.context.unwrap().shuffled);
    assert_eq!(f.full_order().await, names(&["t3", "t4", "t5"]));
}

/// A shuffled play shuffles the whole lane; unshuffling it lands on source
/// order after the track that is playing.
#[tokio::test]
async fn test_shuffled_play_then_unshuffle_yields_source_order() {
    let mut f = release(&["t1", "t2", "t3", "t4", "t5"]).await;
    let played_first = f.play(ReleaseStart::Shuffled).await;

    f.queue.set_shuffle(false).await.unwrap();

    assert_eq!(f.current(), played_first, "the playing track keeps playing");
    let expected: Vec<String> = names(&["t1", "t2", "t3", "t4", "t5"])
        .into_iter()
        .filter(|t| *t != played_first)
        .collect();
    assert_eq!(f.upcoming().await, expected, "source order resumes");
}
