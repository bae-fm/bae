#[test]
fn test_front_peeks_without_consuming() {
    let mut q = queue();
    assert_eq!(q.front(), None);
    q.add_to_queue(rel(&["a", "b"]));
    assert_eq!(q.front(), Some("a"));
    assert_eq!(
        upcoming_tracks(&q),
        vec!["a", "b"],
        "front must not consume"
    );
}

/// The projection keeps the two lanes separate: the manual lane is its own
/// list, the context is its not-yet-played tail, and no manual entry leaks
/// into the context list (or vice versa). A sequential context is not
/// shuffled.
#[test]
fn test_projection_keeps_manual_and_context_separate() {
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
    q.add_to_queue(rel(&["m1", "m2"]));

    let manual: Vec<String> = q.manual_entries().into_iter().map(|e| e.track_id).collect();
    assert_eq!(manual, vec!["m1", "m2"], "the manual lane is its own list");

    let ctx = q.context_projection().expect("a release is playing");
    let context_tracks: Vec<String> = ctx.upcoming.into_iter().map(|e| e.track_id).collect();
    assert_eq!(
        context_tracks,
        vec!["08c7fe07-b56a-4c63-8df6-ad2967fa0653", "t3"],
        "the context is only the not-yet-played tail"
    );
    assert!(!ctx.shuffled, "a sequential context is not shuffled");

    // The lanes don't bleed into each other.
    assert!(
        !context_tracks.iter().any(|t| t == "m1" || t == "m2"),
        "manual entries are not mixed into the context list"
    );
    assert!(
        !manual
            .iter()
            .any(|t| t == "08c7fe07-b56a-4c63-8df6-ad2967fa0653" || t == "t3"),
        "context entries are not mixed into the manual list"
    );
}

/// A shuffled context carries its `shuffled` flag through the projection.
#[test]
fn test_projection_context_carries_shuffled_flag() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
        ]),
        ContextStart::Shuffled { seed: 7 },
    );
    let ctx = q.context_projection().expect("a release is playing");
    assert!(ctx.shuffled, "a shuffled context reports shuffled");
}

/// No context → no projection; the manual lane still projects on its own.
#[test]
fn test_projection_no_context_is_none() {
    let mut q = queue();
    q.add_to_queue(rel(&["m1", "m2"]));
    assert!(
        q.context_projection().is_none(),
        "nothing is playing from a release"
    );
    let manual: Vec<String> = q.manual_entries().into_iter().map(|e| e.track_id).collect();
    assert_eq!(manual, vec!["m1", "m2"]);
}

#[test]
fn test_has_upcoming_and_has_previous() {
    let mut q = queue();
    assert!(!q.has_upcoming());
    assert!(!q.has_previous());
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ]),
        ContextStart::Index(0),
    );
    assert!(q.has_upcoming());
    assert!(!q.has_previous());
    q.next_entry(); // → t2
    assert!(!q.has_upcoming());
    assert!(q.has_previous());
}

// -- next_sequential_context_track (physical-side pause edge) ---------------

/// The only edge that yields a physical side pause: the next track of a
/// sequential release context, with an empty manual lane. All other shapes —
/// no context, a shuffled lane, a pending manual entry, or the last context
/// track — report `None`.
#[test]
fn test_next_sequential_context_track_all_branches() {
    // No context at all → None.
    let mut q = queue();
    assert_eq!(q.next_sequential_context_track(), None);

    // Sequential context with an upcoming track → that track.
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
        ]),
        ContextStart::Index(0),
    );
    assert_eq!(
        q.next_sequential_context_track(),
        Some("08c7fe07-b56a-4c63-8df6-ad2967fa0653")
    );

    // A pending manual entry is not physical-side playback → None.
    q.add_to_queue(rel(&["m1"]));
    assert_eq!(q.next_sequential_context_track(), None);

    // A shuffled lane is not a physical side → None.
    let mut shuffled = queue();
    shuffled.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
        ]),
        ContextStart::Shuffled { seed: 5 },
    );
    assert_eq!(shuffled.next_sequential_context_track(), None);

    // The last track of a sequential context has no upcoming track → None.
    let mut last = queue();
    last.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ]),
        ContextStart::Index(1),
    );
    assert_eq!(last.next_sequential_context_track(), None);
}

/// Shuffling closes the physical-side-pause gate and unshuffling reopens it —
/// a sided release only pauses between sides while the lane is in its own
/// order.
#[test]
fn test_shuffle_closes_the_side_pause_gate_and_unshuffle_reopens_it() {
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
    assert_eq!(
        q.next_sequential_context_track(),
        Some("08c7fe07-b56a-4c63-8df6-ad2967fa0653")
    );

    q.set_shuffle(true, 7);
    assert_eq!(
        q.next_sequential_context_track(),
        None,
        "a shuffled lane has no physical-side edge"
    );

    q.set_shuffle(false, 0);
    assert_eq!(
        q.next_sequential_context_track(),
        Some("08c7fe07-b56a-4c63-8df6-ad2967fa0653"),
        "unshuffling reopens the gate on the restored order"
    );
}

// -- set_shuffle edge cases ------------------------------------------------

/// With nothing playing there is no context to reorder, so `set_shuffle` is a
/// no-op: no context appears and no track becomes current.
#[test]
fn test_set_shuffle_with_no_context_is_noop() {
    let mut q = queue();
    q.set_shuffle(true, 7);
    assert!(q.context_projection().is_none());
    assert_eq!(q.current_track_id(), None);
}

// -- previous_action with a manual current ---------------------------------

/// When the current track is a manual entry (not the context's cursor entry),
/// stepping back within 3s lands on the context's cursor track — the release
/// track that preceded the manual insertion.
#[test]
fn test_previous_action_from_manual_current_lands_on_cursor() {
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
    q.add_to_queue(rel(&["m1"]));
    // Drain the manual lane: current becomes m1 while the cursor stays on t1.
    assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "m1"));
    assert_eq!(q.current_track_id(), Some("m1"));

    // Current is a manual item, so Previous lands on the cursor entry t1.
    assert!(matches!(
        q.previous_action(1000),
        PreviousAction::PlayPrevious(t) if t == "08c7ff07-b56a-4e16-8df6-ae2967fa0806"
    ));
}

// -- remove of the currently-playing context entry -------------------------

/// Removing the context entry that is currently playing clears `current`
/// (nothing is playing until the service advances), and the cursor stays in
/// bounds.
#[test]
fn test_remove_current_context_entry_clears_current() {
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
    // Skip to t2 so it is both the cursor entry and current.
    let t2_id = q.upcoming()[0].id.clone();
    q.skip_to(&t2_id);
    assert_eq!(
        q.current_track_id(),
        Some("08c7fe07-b56a-4c63-8df6-ad2967fa0653")
    );

    let removed = q.remove(&t2_id);
    assert_eq!(
        removed.map(|e| e.track_id),
        Some("08c7fe07-b56a-4c63-8df6-ad2967fa0653".into())
    );
    assert_eq!(
        q.current_track_id(),
        None,
        "removing the playing context entry clears current"
    );
}

// -- revision ---------------------------------------------------------------

/// Every mutating op bumps the revision; reads never do.
#[test]
fn test_revision_bumps_on_mutations_not_reads() {
    let mut q = queue();
    assert_eq!(q.revision(), 0);

    q.add_to_queue(rel(&["a", "b"]));
    assert_eq!(q.revision(), 1, "add_to_queue bumps");

    // Reads never bump.
    let _ = q.upcoming();
    let _ = q.front();
    let _ = q.has_upcoming();
    let _ = q.context_projection();
    let _ = q.manual_entries();
    assert_eq!(q.revision(), 1, "reads never bump");

    let id = manual_ids(&q)[0].clone();
    q.reorder(&id, None);
    assert_eq!(q.revision(), 2, "reorder bumps");

    q.remove(&id);
    assert_eq!(q.revision(), 3, "remove bumps");

    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
        ]),
        ContextStart::Index(0),
    );
    assert_eq!(q.revision(), 4, "play_release bumps");

    q.next_entry();
    assert_eq!(q.revision(), 5, "advancing to the next track bumps");

    q.previous_action(1000);
    assert_eq!(q.revision(), 6, "stepping back bumps");

    let ctx_id = q.upcoming()[0].id.clone();
    q.skip_to(&ctx_id);
    assert_eq!(q.revision(), 7, "skip_to a context entry bumps");

    q.set_shuffle(true, 7);
    assert_eq!(q.revision(), 8, "set_shuffle bumps");

    q.clear_up_next();
    assert_eq!(q.revision(), 9, "clear_up_next bumps");

    q.clear_playing_from();
    assert_eq!(q.revision(), 10, "clear_playing_from bumps");
}

/// Unknown ids and other documented no-ops don't bump the revision.
#[test]
fn test_revision_unchanged_on_noops() {
    let mut q = queue();
    q.add_to_queue(rel(&["a"]));
    let after_add = q.revision();

    assert_eq!(q.remove(&QueueEntryId("nope".into())), None);
    assert_eq!(q.revision(), after_add, "unknown remove id doesn't bump");

    q.reorder(&QueueEntryId("nope".into()), None);
    assert_eq!(
        q.revision(),
        after_add,
        "unknown reorder source doesn't bump"
    );

    assert_eq!(q.skip_to(&QueueEntryId("nope".into())), None);
    assert_eq!(q.revision(), after_add, "unknown skip_to id doesn't bump");

    q.set_shuffle(true, 1);
    assert_eq!(
        q.revision(),
        after_add,
        "set_shuffle with no playing context doesn't bump"
    );

    q.clear_playing_from();
    assert_eq!(
        q.revision(),
        after_add,
        "clear_playing_from with no context doesn't bump"
    );
}

// -- context reorder to the end --------------------------------------------

/// Reordering a context entry with `before = None` moves it to the end of the
/// context order while the cursor stays on the playing track.
#[test]
fn test_reorder_context_to_end() {
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
    let t2_id = q.upcoming()[0].id.clone(); // upcoming = [t2, t3, t4]
    q.reorder(&t2_id, None);
    assert_eq!(
        upcoming_tracks(&q),
        vec!["t3", "t4", "08c7fe07-b56a-4c63-8df6-ad2967fa0653"]
    );
    assert_eq!(
        q.current_track_id(),
        Some("08c7ff07-b56a-4e16-8df6-ae2967fa0806"),
        "the cursor stays on the playing track"
    );
}
