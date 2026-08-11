/// A sequential restore rebuilds source order with the cursor on the track
/// that was playing, so the history behind it is the source prefix and
/// Previous works. The session's edits are deliberately not in the recipe.
#[test]
fn test_snapshot_restore_sequential_context() {
    let mut q = queue();
    q.play_release(
        rel_src("rel-A"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
        ]),
        ContextStart::Index(1),
    );
    let snap = q.snapshot();
    assert_eq!(
        snap.context.as_ref().unwrap().source,
        ContextSource::Release("rel-A".into())
    );
    assert!(!snap.context.as_ref().unwrap().shuffled);
    assert_eq!(
        snap.current_track_id.as_deref(),
        Some("08c7fe07-b56a-4c63-8df6-ad2967fa0653")
    );

    let mut restored = queue();
    restored.restore(
        snap,
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
        ]),
        0,
    );
    assert_eq!(
        restored.current_track_id(),
        Some("08c7fe07-b56a-4c63-8df6-ad2967fa0653")
    );
    assert_eq!(upcoming_tracks(&restored), vec!["t3"]);
    assert!(
        restored.has_previous(),
        "the cursor landed on t2's source position, so t1 is behind it"
    );
}

/// A shuffled restore puts the playing track first with the rest freshly
/// permuted behind it: the shuffled order and the history do not survive a
/// restart, and unshuffling afterwards lands on source order.
#[test]
fn test_snapshot_restore_shuffled_fronts_the_current_track() {
    let mut q = queue();
    q.play_release(
        rel_src("rel-A"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
            "t5",
        ]),
        ContextStart::Shuffled { seed: 99 },
    );
    q.next_entry();
    q.next_entry();
    let current_before = q.current_track_id().unwrap().to_string();

    let mut restored = queue();
    restored.restore(
        q.snapshot(),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
            "t5",
        ]),
        5,
    );

    assert_eq!(restored.current_track_id().unwrap(), current_before);
    assert!(
        restored.context_projection().unwrap().shuffled,
        "the lane comes back shuffled"
    );
    assert!(
        !restored.has_previous(),
        "the current track is first, so nothing is behind it"
    );
    let mut rest = upcoming_tracks(&restored);
    rest.sort();
    let expected: Vec<&str> = [
        "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
        "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        "t3",
        "t4",
        "t5",
    ]
    .into_iter()
    .filter(|t| *t != current_before)
    .collect();
    assert_eq!(rest, expected, "the rest of the source is behind it");

    restored.set_shuffle(false, 0);
    assert_eq!(
        upcoming_tracks(&restored),
        expected,
        "unshuffling after a restart yields source order"
    );
}

/// A recipe whose current track the source no longer holds can't be resumed:
/// the context drops and the track resumes standalone.
#[test]
fn test_snapshot_restore_drops_a_context_missing_its_current_track() {
    let snapshot = QueueSnapshot {
        context: Some(ContextSnapshot {
            source: rel_src("r1"),
            shuffled: false,
        }),
        manual: vec!["m1".into()],
        current_track_id: Some("ghost".into()),
        repeat: RepeatMode::Off,
    };
    let mut q = queue();
    q.restore(
        snapshot,
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ]),
        0,
    );

    assert_eq!(q.current_track_id(), Some("ghost"));
    assert!(
        q.context_projection().is_none(),
        "the context is dropped rather than cued at a track nobody was playing"
    );
    assert_eq!(upcoming_tracks(&q), vec!["m1"], "the manual lane survives");
}

#[test]
fn test_snapshot_restore_single_track_with_manual_lane() {
    let mut q = queue();
    q.play_single("solo".into());
    q.add_to_queue(rel(&["m1", "m2"]));
    let snap = q.snapshot();
    assert!(snap.context.is_none());

    let mut restored = queue();
    restored.restore(snap, vec![], 0);
    assert_eq!(restored.current_track_id(), Some("solo"));
    assert_eq!(upcoming_tracks(&restored), vec!["m1", "m2"]);
}

#[test]
fn test_manual_drains_before_context() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ]),
        ContextStart::Index(0),
    );
    q.add_to_queue(rel(&["m1"]));
    // current = t1; next drains manual (m1) before advancing the context.
    assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "m1"));
    assert!(
        matches!(q.next_entry(), NextEntry::Play(t) if t == "08c7fe07-b56a-4c63-8df6-ad2967fa0653")
    );
    assert!(matches!(q.next_entry(), NextEntry::Stop));
}

#[test]
fn test_context_advances_by_cursor() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
        ]),
        ContextStart::Index(0),
    );
    assert!(
        matches!(q.next_entry(), NextEntry::Play(t) if t == "08c7fe07-b56a-4c63-8df6-ad2967fa0653")
    );
    assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "t3"));
    assert!(matches!(q.next_entry(), NextEntry::Stop));
}

#[test]
fn test_context_repeat_loops_from_stored_order() {
    // The queue holds the context order, so looping reuses it: the queue has
    // no library access, so it structurally cannot re-fetch.
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ]),
        ContextStart::Index(0),
    );
    q.set_repeat_mode(RepeatMode::Context);
    assert!(
        matches!(q.next_entry(), NextEntry::Play(t) if t == "08c7fe07-b56a-4c63-8df6-ad2967fa0653")
    );
    // Exhausted under Context repeat → loop from the start of the same order.
    assert!(
        matches!(q.next_entry(), NextEntry::Play(t) if t == "08c7ff07-b56a-4e16-8df6-ae2967fa0806")
    );
    assert!(
        matches!(q.next_entry(), NextEntry::Play(t) if t == "08c7fe07-b56a-4c63-8df6-ad2967fa0653")
    );
}

#[test]
fn test_repeat_track_pins_current() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ]),
        ContextStart::Index(0),
    );
    q.set_repeat_mode(RepeatMode::Track);
    assert!(
        matches!(q.next_entry(), NextEntry::RepeatCurrent(t) if t == "08c7ff07-b56a-4e16-8df6-ae2967fa0806")
    );
}

#[test]
fn test_previous_steps_cursor_back_multiple() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
        ]),
        ContextStart::Index(0),
    );
    q.next_entry(); // t2
    q.next_entry(); // t3
    assert!(
        matches!(q.previous_action(1000), PreviousAction::PlayPrevious(t) if t == "08c7fe07-b56a-4c63-8df6-ad2967fa0653")
    );
    assert!(
        matches!(q.previous_action(1000), PreviousAction::PlayPrevious(t) if t == "08c7ff07-b56a-4e16-8df6-ae2967fa0806")
    );
    // At the context start, Previous restarts.
    assert!(matches!(
        q.previous_action(1000),
        PreviousAction::RestartCurrent
    ));
}

#[test]
fn test_previous_past_3s_restarts() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ]),
        ContextStart::Index(1),
    );
    assert!(matches!(
        q.previous_action(5000),
        PreviousAction::RestartCurrent
    ));
}

#[test]
fn test_skip_to_context_tail_moves_cursor() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
        ]),
        ContextStart::Index(0),
    );
    let t3_id = q.upcoming()[1].id.clone(); // upcoming = [t2, t3, t4]
    let entry = q.skip_to(&t3_id);
    assert_eq!(entry.map(|e| e.track_id), Some("t3".into()));
    assert_eq!(q.current_track_id(), Some("t3"));
    assert_eq!(upcoming_tracks(&q), vec!["t4"]);
}

#[test]
fn test_remove_context_tail_entry() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
        ]),
        ContextStart::Index(0),
    );
    let t2_id = q.upcoming()[0].id.clone();
    let removed = q.remove(&t2_id);
    assert_eq!(
        removed.map(|e| e.track_id),
        Some("08c7fe07-b56a-4c63-8df6-ad2967fa0653".into())
    );
    assert_eq!(upcoming_tracks(&q), vec!["t3"]);
}

#[test]
fn test_reorder_context_tail_keeps_cursor() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
        ]),
        ContextStart::Index(0),
    );
    let up = q.upcoming(); // [t2, t3, t4]
    let t2_id = up[0].id.clone();
    let t3_id = up[1].id.clone();
    // Move t3 before t2 → upcoming becomes [t3, t2, t4].
    q.reorder(&t3_id, Some(&t2_id));
    assert_eq!(
        upcoming_tracks(&q),
        vec!["t3", "08c7fe07-b56a-4c63-8df6-ad2967fa0653", "t4"]
    );
    assert_eq!(
        q.current_track_id(),
        Some("08c7ff07-b56a-4e16-8df6-ae2967fa0806"),
        "the cursor stays on the playing track"
    );
}

#[test]
fn test_reorder_cross_lane_is_noop() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ]),
        ContextStart::Index(0),
    );
    q.add_to_queue(rel(&["m1"]));
    let manual_id = manual_ids(&q)[0].clone();
    let context_id = q
        .upcoming()
        .into_iter()
        .find(|e| e.track_id == "08c7fe07-b56a-4c63-8df6-ad2967fa0653")
        .unwrap()
        .id;
    // Manual source, context target → no-op (can't cross lanes).
    q.reorder(&manual_id, Some(&context_id));
    assert_eq!(
        upcoming_tracks(&q),
        vec!["m1", "08c7fe07-b56a-4c63-8df6-ad2967fa0653"]
    );
}

#[test]
fn test_insert_at_middle() {
    let mut q = queue();
    q.add_to_queue(rel(&["a", "b", "c"]));
    q.insert_at(1, rel(&["x", "y"]));
    assert_eq!(upcoming_tracks(&q), vec!["a", "x", "y", "b", "c"]);
}

#[test]
fn test_insert_at_beyond_end_clamps() {
    let mut q = queue();
    q.add_to_queue(rel(&["a"]));
    q.insert_at(999, rel(&["x"]));
    assert_eq!(upcoming_tracks(&q), vec!["a", "x"]);
}

// -- remove_by_ids (library deletion) --------------------------------------

#[test]
fn test_remove_by_ids_clears_manual_and_context() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&["08c7ff07-b56a-4e16-8df6-ae2967fa0806", "dup", "t3"]),
        ContextStart::Index(0),
    );
    q.add_to_queue(rel(&["dup", "m2"]));
    let ids: HashSet<String> = ["dup"].iter().map(|s| s.to_string()).collect();
    q.remove_by_ids(&ids);
    // Manual "dup" gone (m2 stays); context "dup" gone (t1, t3 stay).
    assert_eq!(upcoming_tracks(&q), vec!["m2", "t3"]);
}

#[test]
fn test_remove_by_ids_keeps_cursor_on_same_track() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&["gone", "08c7fe07-b56a-4c63-8df6-ad2967fa0653", "t3"]),
        ContextStart::Index(1),
    );
    // current = t2 (cursor 1). Deleting t1 (before cursor) keeps current at t2.
    let ids: HashSet<String> = ["gone"].iter().map(|s| s.to_string()).collect();
    q.remove_by_ids(&ids);
    assert_eq!(
        q.current_track_id(),
        Some("08c7fe07-b56a-4c63-8df6-ad2967fa0653")
    );
    assert_eq!(upcoming_tracks(&q), vec!["t3"]);
}

#[test]
fn test_remove_by_ids_clears_current_when_deleted() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ]),
        ContextStart::Index(0),
    );
    let ids: HashSet<String> = ["08c7ff07-b56a-4e16-8df6-ae2967fa0806"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    q.remove_by_ids(&ids);
    assert_eq!(q.current_track_id(), None);
}

#[test]
fn test_remove_by_ids_deleting_current_last_entry_keeps_cursor_valid() {
    let mut q = queue();
    // current = t2 at the last position (cursor == len-1).
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ]),
        ContextStart::Index(1),
    );
    let ids: HashSet<String> = ["08c7fe07-b56a-4c63-8df6-ad2967fa0653"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    q.remove_by_ids(&ids);
    assert_eq!(
        q.current_track_id(),
        None,
        "the deleted playing track clears current"
    );
    // The cursor must not be stranded at == len: Previous must not panic.
    assert!(matches!(
        q.previous_action(1000),
        PreviousAction::PlayPrevious(t) if t == "08c7ff07-b56a-4e16-8df6-ae2967fa0806"
    ));
}
