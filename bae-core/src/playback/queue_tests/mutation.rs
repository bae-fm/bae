#[test]
fn test_add_to_queue_fills_manual_lane() {
    let mut q = queue();
    q.add_to_queue(rel(&["a", "b", "c"]));
    assert_eq!(upcoming_tracks(&q), vec!["a", "b", "c"]);
}

#[test]
fn test_add_to_queue_mints_distinct_ids() {
    let mut q = queue();
    q.add_to_queue(rel(&["a", "a"]));
    let ids = manual_ids(&q);
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1], "duplicate tracks get distinct ids");
}

#[test]
fn test_add_next_preserves_order() {
    let mut q = queue();
    q.add_to_queue(rel(&["x"]));
    q.add_next(rel(&["a", "b"]));
    assert_eq!(upcoming_tracks(&q), vec!["a", "b", "x"]);
}

#[test]
fn test_remove_by_entry_id() {
    let mut q = queue();
    q.add_to_queue(rel(&["a", "b", "c"]));
    let b_id = manual_ids(&q)[1].clone();
    let removed = q.remove(&b_id);
    assert_eq!(removed.map(|e| e.track_id), Some("b".into()));
    assert_eq!(upcoming_tracks(&q), vec!["a", "c"]);
}

/// The load-bearing dup test: the same track enqueued twice, removing one
/// instance by its id leaves the other instance — and its id — intact.
#[test]
fn test_remove_one_duplicate_keeps_the_other() {
    let mut q = queue();
    q.add_to_queue(rel(&["dup", "dup"]));
    let ids = manual_ids(&q);
    let removed = q.remove(&ids[0]).expect("first instance removed");
    assert_eq!(removed.id, ids[0]);

    let remaining = manual_ids(&q);
    assert_eq!(remaining.len(), 1, "exactly one instance remains");
    assert_eq!(remaining[0], ids[1], "the other instance's id survives");
}

#[test]
fn test_remove_unknown_id_is_noop() {
    let mut q = queue();
    q.add_to_queue(rel(&["a"]));
    assert_eq!(q.remove(&QueueEntryId("nope".into())), None);
    assert_eq!(upcoming_tracks(&q), vec!["a"]);
}

#[test]
fn test_reorder_forward() {
    let mut q = queue();
    q.add_to_queue(rel(&["a", "b", "c", "d"]));
    let ids = manual_ids(&q);
    q.reorder(&ids[0], Some(&ids[2]));
    assert_eq!(upcoming_tracks(&q), vec!["b", "a", "c", "d"]);
}

#[test]
fn test_reorder_to_end() {
    let mut q = queue();
    q.add_to_queue(rel(&["a", "b", "c", "d"]));
    let ids = manual_ids(&q);
    q.reorder(&ids[0], None);
    assert_eq!(upcoming_tracks(&q), vec!["b", "c", "d", "a"]);
}

#[test]
fn test_reorder_backward() {
    let mut q = queue();
    q.add_to_queue(rel(&["a", "b", "c", "d"]));
    let ids = manual_ids(&q);
    q.reorder(&ids[2], Some(&ids[0]));
    assert_eq!(upcoming_tracks(&q), vec!["c", "a", "b", "d"]);
}

#[test]
fn test_reorder_before_self_is_noop() {
    let mut q = queue();
    q.add_to_queue(rel(&["a", "b", "c"]));
    let ids = manual_ids(&q);
    q.reorder(&ids[1], Some(&ids[1]));
    assert_eq!(upcoming_tracks(&q), vec!["a", "b", "c"]);
}

#[test]
fn test_reorder_unknown_source_is_noop() {
    let mut q = queue();
    q.add_to_queue(rel(&["a", "b"]));
    q.reorder(&QueueEntryId("nope".into()), None);
    assert_eq!(upcoming_tracks(&q), vec!["a", "b"]);
}

#[test]
fn test_clear_up_next_empties_manual_keeps_context() {
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
    q.clear_up_next();
    // Manual gone; context tail (t2, t3) survives.
    assert_eq!(
        upcoming_tracks(&q),
        vec!["08c7fe07-b56a-4c63-8df6-ad2967fa0653", "t3"]
    );
}

/// Clearing the context lane drops its rows, its history, and its label
/// while the playing track keeps playing — the lane it came from is gone,
/// not the audio.
#[test]
fn test_clear_playing_from_drops_the_context_keeping_the_current_track() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
        ]),
        ContextStart::Index(1),
    );
    assert!(q.has_previous(), "t1 sits behind the cursor");

    q.clear_playing_from();

    assert_eq!(
        q.current_track_id(),
        Some("08c7fe07-b56a-4c63-8df6-ad2967fa0653"),
        "the playing track keeps playing"
    );
    assert!(
        q.context_projection().is_none(),
        "the context section is gone"
    );
    assert!(!q.has_previous(), "its history went with it");
    assert!(upcoming_tracks(&q).is_empty());
}

/// After clearing the context lane, Up Next drains and then playback stops —
/// there is no lane left to fall through to.
#[test]
fn test_clear_playing_from_then_up_next_drains_and_stops() {
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

    q.clear_playing_from();

    assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "m1"));
    assert!(matches!(q.next_entry(), NextEntry::Stop));
}

#[test]
fn test_clear_playing_from_with_no_context_is_noop() {
    let mut q = queue();
    q.add_to_queue(rel(&["m1"]));
    let revision = q.revision();

    q.clear_playing_from();

    assert_eq!(
        q.revision(),
        revision,
        "nothing to clear, so the projection didn't change"
    );
    assert_eq!(upcoming_tracks(&q), vec!["m1"]);
}

#[test]
fn test_skip_to_manual_drains_prefix() {
    let mut q = queue();
    q.add_to_queue(rel(&["a", "b", "c", "d"]));
    let c_id = manual_ids(&q)[2].clone();
    let entry = q.skip_to(&c_id);
    assert_eq!(entry.map(|e| e.track_id), Some("c".into()));
    assert_eq!(upcoming_tracks(&q), vec!["d"]);
    assert_eq!(q.current_track_id(), Some("c"));
}

#[test]
fn test_skip_to_unknown_id_is_noop() {
    let mut q = queue();
    q.add_to_queue(rel(&["a"]));
    assert_eq!(q.skip_to(&QueueEntryId("nope".into())), None);
    assert_eq!(upcoming_tracks(&q), vec!["a"]);
}

// -- context ---------------------------------------------------------------

#[test]
fn test_play_release_sets_current_and_upcoming() {
    let mut q = queue();
    let first = q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
        ]),
        ContextStart::Index(0),
    );
    assert_eq!(first, "08c7ff07-b56a-4e16-8df6-ae2967fa0806");
    assert_eq!(
        q.current_track_id(),
        Some("08c7ff07-b56a-4e16-8df6-ae2967fa0806")
    );
    assert_eq!(
        upcoming_tracks(&q),
        vec!["08c7fe07-b56a-4c63-8df6-ad2967fa0653", "t3"]
    );
}

#[test]
fn test_play_release_start_index() {
    let mut q = queue();
    let first = q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
        ]),
        ContextStart::Index(1),
    );
    assert_eq!(first, "08c7fe07-b56a-4c63-8df6-ad2967fa0653");
    assert_eq!(upcoming_tracks(&q), vec!["t3"]);
}

/// Up Next is the user's own arrangement: filling the context lane leaves it
/// alone, and it still drains before the newly filled context.
#[test]
fn test_play_release_leaves_up_next_intact() {
    let mut q = queue();
    q.add_to_queue(rel(&["m1", "m2"]));

    let first = q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
        ]),
        ContextStart::Index(0),
    );

    assert_eq!(first, "08c7ff07-b56a-4e16-8df6-ae2967fa0806");
    assert_eq!(
        upcoming_tracks(&q),
        vec!["m1", "m2", "08c7fe07-b56a-4c63-8df6-ad2967fa0653", "t3"],
        "Up Next survives the fill and drains first"
    );
    assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "m1"));
    assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "m2"));
    assert!(
        matches!(q.next_entry(), NextEntry::Play(t) if t == "08c7fe07-b56a-4c63-8df6-ad2967fa0653")
    );
}

#[test]
fn test_play_single_leaves_up_next_intact() {
    let mut q = queue();
    q.add_to_queue(rel(&["m1", "m2"]));

    q.play_single("solo".into());

    assert_eq!(q.current_track_id(), Some("solo"));
    assert_eq!(upcoming_tracks(&q), vec!["m1", "m2"]);
    assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "m1"));
}

#[test]
fn test_play_release_shuffled_keeps_all_tracks() {
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
    let mut all = full_order(&q);
    all.sort();
    // Sorted, so the ids come out in lexical order.
    assert_eq!(
        all,
        vec![
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "t3",
            "t4"
        ]
    );
}

/// A repeating shuffled lane loops a freshly permuted order each pass, not
/// the same order every time. Both passes' orders are read from the queue (no
/// re-implementation of the shuffle) and are deterministic for a fixed seed.
#[test]
fn test_context_repeat_shuffled_loops_a_re_derived_order() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
            "t5",
        ]),
        ContextStart::Shuffled { seed: 1 },
    );
    q.set_repeat_mode(RepeatMode::Context);
    let first_pass = full_order(&q);
    // Advance to the end of the pass, then one more advance loops it.
    for _ in 0..first_pass.len() - 1 {
        q.next_entry();
    }
    match q.next_entry() {
        NextEntry::Play(_) => {}
        other => panic!("expected a looped Play, got {other:?}"),
    }
    let second_pass = full_order(&q);

    let (mut a, mut b) = (first_pass.clone(), second_pass.clone());
    a.sort();
    b.sort();
    assert_eq!(a, b, "the loop replays exactly the same tracks");
    assert_ne!(
        first_pass, second_pass,
        "but in a freshly re-derived order each pass"
    );
}

/// Stamping the WHOLE lane (not just the upcoming tail) is what keeps
/// unshuffle well-defined after a repeat wrap has moved played rows back into
/// upcoming: the post-wrap unshuffle lands every upcoming row in the stamp's
/// relative order.
#[test]
fn test_unshuffle_after_a_repeat_wrap_lands_in_the_stamped_order() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
            "t5",
        ]),
        ContextStart::Index(0),
    );
    // Shuffle on from the head, so the stamp is source order.
    q.set_shuffle(true, 7);
    q.set_repeat_mode(RepeatMode::Context);
    // Play through the pass; the next advance wraps and re-permutes the lane.
    for _ in 0..4 {
        q.next_entry();
    }
    assert!(matches!(q.next_entry(), NextEntry::Play(_)));

    q.set_shuffle(false, 0);

    let wrapped_current = q.current_track_id().unwrap().to_string();
    let expected: Vec<&str> = [
        "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
        "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        "t3",
        "t4",
        "t5",
    ]
    .into_iter()
    .filter(|t| *t != wrapped_current)
    .collect();
    assert_eq!(
        upcoming_tracks(&q),
        expected,
        "every row after the wrap's cursor sits in the stamped order"
    );
}

/// A sequential lane under `Context` repeat wraps to row 0 in the same order,
/// carrying the session's removals and reorders into the next pass.
#[test]
fn test_context_repeat_sequential_wrap_carries_edits_into_the_next_pass() {
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
    // Remove t2 and move t4 ahead of t3: the lane becomes [t1, t4, t3].
    let up = q.upcoming();
    let t2_id = up[0].id.clone();
    let t3_id = up[1].id.clone();
    let t4_id = up[2].id.clone();
    q.remove(&t2_id);
    q.reorder(&t4_id, Some(&t3_id));
    assert_eq!(
        q.context_order(),
        vec!["08c7ff07-b56a-4e16-8df6-ae2967fa0806", "t4", "t3"]
    );

    q.set_repeat_mode(RepeatMode::Context);
    q.next_entry(); // t4
    q.next_entry(); // t3
    assert!(
        matches!(q.next_entry(), NextEntry::Play(t) if t == "08c7ff07-b56a-4e16-8df6-ae2967fa0806")
    );
    assert_eq!(
        q.context_order(),
        vec!["08c7ff07-b56a-4e16-8df6-ae2967fa0806", "t4", "t3"],
        "the wrap replays the edited lane, not the source"
    );
}

/// A `Library` source is the same construct as a release: its tracks
/// materialize into the context under the seed, keeping every track, and the
/// snapshot reports the `Library` source.
#[test]
fn test_library_source_context_materializes_all_tracks() {
    let mut q = queue();
    q.play_release(
        ContextSource::Library,
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
            "t5",
        ]),
        ContextStart::Shuffled { seed: 3 },
    );
    let mut all = full_order(&q);
    all.sort();
    // Sorted, so the ids come out in lexical order.
    assert_eq!(
        all,
        vec![
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "t3",
            "t4",
            "t5"
        ]
    );
    assert_eq!(q.snapshot().context.unwrap().source, ContextSource::Library);
}

// -- shuffle toggle --------------------------------------------------------

#[test]
fn test_set_shuffle_on_keeps_current_track_with_cursor_on_it() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
            "t5",
        ]),
        ContextStart::Index(2),
    );
    assert_eq!(q.current_track_id(), Some("t3"));

    q.set_shuffle(true, 7);

    // The playing track keeps playing; the cursor sits on it.
    assert_eq!(q.current_track_id(), Some("t3"));
    let ctx = q.context_projection().expect("a release is playing");
    assert!(
        ctx.shuffled,
        "the context reports shuffled after turning shuffle on"
    );

    // Every row is retained in the new order, just re-ordered.
    let mut all = q.context_order();
    all.sort();
    // Sorted, so the ids come out in lexical order.
    assert_eq!(
        all,
        vec![
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "t3",
            "t4",
            "t5"
        ],
        "no track is lost"
    );
}

/// Turning shuffle on is surgery on the upcoming tail alone: the current row
/// and the history before it stay exactly where they were, and every unplayed
/// row stays upcoming — none is stranded behind the cursor where Next never
/// reaches. Checked across seeds, since a permutation that happens to fix a
/// row in place would hide the bug.
#[test]
fn test_set_shuffle_on_keeps_history_and_permutes_only_upcoming() {
    for seed in 0..8 {
        let mut q = queue();
        q.play_release(
            rel_src("r1"),
            rel(&[
                "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
                "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
                "t3",
                "t4",
                "t5",
            ]),
            ContextStart::Index(2),
        );
        q.set_shuffle(true, seed);

        assert_eq!(q.current_track_id(), Some("t3"), "seed {seed}");
        assert_eq!(
            q.context_order()[..3],
            [
                "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
                "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
                "t3"
            ],
            "seed {seed}: the history and the current row never move"
        );
        let mut rest = upcoming_tracks(&q);
        rest.sort();
        assert_eq!(
            rest,
            vec!["t4", "t5"],
            "seed {seed}: every unplayed row stays upcoming"
        );
    }
}

/// The lane is the authority: a reorder made before shuffling round-trips
/// exactly, because the stamp taken when shuffle turns on is lane order — not
/// album order.
#[test]
fn test_shuffle_round_trip_restores_a_reordered_lane_not_source_order() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
            "t5",
        ]),
        ContextStart::Index(0),
    );
    // Move t5 to the front of the upcoming tail: [t2, t3, t4, t5] → [t5, t2, t3, t4].
    let up = q.upcoming();
    let t5_id = up[3].id.clone();
    let t2_id = up[0].id.clone();
    q.reorder(&t5_id, Some(&t2_id));
    let reordered = upcoming_tracks(&q);
    assert_eq!(
        reordered,
        vec!["t5", "08c7fe07-b56a-4c63-8df6-ad2967fa0653", "t3", "t4"]
    );

    q.set_shuffle(true, 7);
    q.set_shuffle(false, 0);

    assert_eq!(
        upcoming_tracks(&q),
        reordered,
        "unshuffling lands the lane back in ITS order, not the album's"
    );
}

/// Shuffle on then off with no edits in between round-trips to the original
/// lane order.
#[test]
fn test_shuffle_round_trip_with_no_edits_is_the_identity() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
            "t5",
            "t6",
        ]),
        ContextStart::Index(1),
    );
    let before = q.context_order();

    q.set_shuffle(true, 42);
    q.set_shuffle(false, 0);

    assert_eq!(q.context_order(), before);
    assert_eq!(
        q.current_track_id(),
        Some("08c7fe07-b56a-4c63-8df6-ad2967fa0653")
    );
}

/// A row removed while shuffled is gone for the rest of the session — no
/// later operation resurrects it, because no later operation consults
/// anything but the rows. Every other unplayed row keeps its place in line.
#[test]
fn test_a_row_removed_while_shuffled_is_absent_after_unshuffling() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
            "t5",
        ]),
        ContextStart::Index(0),
    );
    q.set_shuffle(true, 7);
    let removed_track = q.upcoming()[0].track_id.clone();
    let removed_id = q.upcoming()[0].id.clone();
    q.remove(&removed_id);

    q.set_shuffle(false, 0);

    assert_eq!(
        q.current_track_id(),
        Some("08c7ff07-b56a-4e16-8df6-ae2967fa0806")
    );
    let expected: Vec<&str> = ["08c7fe07-b56a-4c63-8df6-ad2967fa0653", "t3", "t4", "t5"]
        .into_iter()
        .filter(|t| *t != removed_track)
        .collect();
    assert_eq!(
        upcoming_tracks(&q),
        expected,
        "the removed row is absent; the survivors keep their place in line"
    );
}

/// `set_shuffle` names the state it wants, so asking for the state the lane is
/// already in changes nothing — no re-permutation, no revision bump.
#[test]
fn test_set_shuffle_to_the_current_state_is_idempotent() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
            "t5",
        ]),
        ContextStart::Index(0),
    );
    q.set_shuffle(true, 7);
    let order = q.context_order();
    let revision = q.revision();

    q.set_shuffle(true, 99);

    assert_eq!(q.context_order(), order, "the lane is not re-permuted");
    assert_eq!(
        q.revision(),
        revision,
        "an already-shuffled lane doesn't bump"
    );

    q.set_shuffle(false, 0);
    let sequential_revision = q.revision();
    q.set_shuffle(false, 0);
    assert_eq!(
        q.revision(),
        sequential_revision,
        "an already-sequential lane doesn't bump"
    );
}

#[test]
fn test_set_shuffle_off_restores_source_order_from_current() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
            "t5",
        ]),
        ContextStart::Index(2),
    );
    assert_eq!(q.current_track_id(), Some("t3"));

    // On then off; the playing track rides through to a restored source order.
    q.set_shuffle(true, 7);
    assert!(q.context_projection().unwrap().shuffled);
    assert_eq!(q.current_track_id(), Some("t3"));

    q.set_shuffle(false, 0);

    // Same playing track; the order is back to source order, cursor on it.
    assert_eq!(q.current_track_id(), Some("t3"));
    let ctx = q.context_projection().expect("a release is playing");
    assert!(
        !ctx.shuffled,
        "the context is sequential after turning shuffle off"
    );
    assert_eq!(
        full_order(&q),
        vec!["t3", "t4", "t5"],
        "source order resumes from the current track"
    );
}

/// A shuffled fill permutes the whole lane; unshuffling it lands on source
/// order from the track that is playing.
#[test]
fn test_shuffled_fill_then_unshuffle_yields_source_order() {
    let mut q = queue();
    q.play_release(
        rel_src("r1"),
        rel(&[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
            "t3",
            "t4",
            "t5",
        ]),
        ContextStart::Shuffled { seed: 7 },
    );
    let played_first = q.current_track_id().unwrap().to_string();

    q.set_shuffle(false, 0);

    assert_eq!(
        q.current_track_id().unwrap(),
        played_first,
        "the playing track keeps playing"
    );
    let expected: Vec<&str> = [
        "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
        "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        "t3",
        "t4",
        "t5",
    ]
    .into_iter()
    .filter(|t| *t != played_first)
    .collect();
    assert_eq!(upcoming_tracks(&q), expected, "source order resumes");
}

// -- persistence round-trips -----------------------------------------------
