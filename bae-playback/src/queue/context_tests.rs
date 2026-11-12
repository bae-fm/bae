use super::*;
use crate::queue::tests::*;
use crate::tables::{ContextFill, ReleaseStart, TracksStart};
use crate::ContextSource;

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
    assert_eq!(missing, crate::tables::ContextFilled::StartMissing);
    let empty = f
        .queue
        .play(ContextFill::Release {
            release_id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
            start: ReleaseStart::First,
        })
        .await
        .unwrap();
    assert_eq!(empty, crate::tables::ContextFilled::NoTracks);

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
    assert!(f.value().await.shuffled);
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

/// A `Library` play holds every library track, shuffled, and its source is
/// the library; unshuffling puts it in the library's own order.
#[tokio::test]
async fn test_library_play_holds_all_tracks_in_library_order() {
    let mut f = fixture(&[("r1", &["a1", "a2"]), ("r2", &["b1", "b2", "b3"])]).await;
    let first = f.play_fill(ContextFill::LibraryShuffled).await;
    assert_eq!(sorted(f.full_order().await), names(&["a1", "a2", "b1", "b2", "b3"]));
    let value = f.value().await;
    assert!(value.shuffled);
    let context = value.context.unwrap();
    assert_eq!(context.source, ContextSource::Library);

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
        assert!(f.value().await.shuffled);
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
    let expected: Vec<String> =
        names(&["t2", "t3", "t4", "t5"]).into_iter().filter(|t| *t != removed_track).collect();
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
    assert!(!f.value().await.shuffled);
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
    let expected: Vec<String> =
        names(&["t1", "t2", "t3", "t4", "t5"]).into_iter().filter(|t| *t != played_first).collect();
    assert_eq!(f.upcoming().await, expected, "source order resumes");
}

/// A release named twice in a multi-release play plays twice: each of its
/// tracks is two entries of the context lane, played in turn.
#[tokio::test]
async fn test_a_release_named_twice_plays_twice() {
    let mut f = fixture(&[("r1", &["a1", "a2"]), ("r2", &["b1"])]).await;
    f.play_fill(ContextFill::Releases(vec![f.id("r1"), f.id("r2"), f.id("r1")])).await;
    assert_eq!(f.context_order().await, names(&["a1", "a2", "b1", "a1", "a2"]));
    assert_eq!(f.value().await.context.unwrap().source, ContextSource::Releases);
    let mut played = vec![f.current()];
    while let Some(next) = f.next().await {
        played.push(next);
    }
    assert_eq!(played, names(&["a1", "a2", "b1", "a1", "a2"]));
}

/// A multi-release play concatenates each release's tracks in the input order the
/// releases were chosen, and reports the releases that contributed as the source.
#[tokio::test]
async fn play_releases_concatenates_tracks_in_input_order() {
    let mut f = fixture(&[("two", &["t1", "t2"]), ("one", &["t3"])]).await;
    f.play_fill(ContextFill::Releases(vec![f.id("one"), f.id("two")])).await;
    assert_eq!(f.value().await.context.unwrap().source, ContextSource::Releases);
    assert_eq!(f.context_order().await, names(&["t3", "t1", "t2"]));
}

/// A release with no tracks (deleted, or never existed) is skipped and named
/// as left out; the remaining releases still play in order.
#[tokio::test]
async fn play_releases_skips_a_release_without_tracks() {
    let mut f = fixture(&[("two", &["t1"]), ("one", &["t2", "t3"])]).await;
    let missing = "6f9a1c2e-0b3d-4e5f-8a7b-9c0d1e2f3a4b".to_string();
    let filled = f
        .queue
        .play(ContextFill::Releases(vec![f.id("two"), missing.clone(), f.id("one")]))
        .await
        .unwrap();
    let crate::tables::ContextFilled::Playing { left_out, .. } = filled else {
        panic!("the releases play: {filled:?}");
    };
    assert_eq!(left_out, crate::tables::LeftOut::Releases(vec![missing]));
    assert_eq!(f.value().await.context.unwrap().source, ContextSource::Releases);
    assert_eq!(f.context_order().await, names(&["t1", "t2", "t3"]));
}

/// With nothing playing, shuffle is a mode of its own: turning it on or off
/// records it, and no context appears and no track becomes current.
#[tokio::test]
async fn test_set_shuffle_with_no_context_records_the_mode() {
    let mut f = release(&["t1"]).await;
    assert!(!f.value().await.shuffled);

    f.queue.set_shuffle(true).await.unwrap();
    assert!(f.value().await.shuffled);
    assert!(f.value().await.context.is_none());
    assert_eq!(f.current_name(), None);

    f.queue.set_shuffle(false).await.unwrap();
    assert!(!f.value().await.shuffled);
}

/// The next play starts shuffled when the mode was turned on with nothing
/// playing: the whole lane shuffles from a play in order, and a play from a
/// track starts with it and shuffles the rest.
#[tokio::test]
async fn test_a_play_starts_shuffled_when_the_mode_is_on() {
    let names_in_order = ["t1", "t2", "t3", "t4", "t5", "t6", "t7", "t8"];
    let mut f = release(&names_in_order).await;
    f.queue.set_shuffle(true).await.unwrap();

    f.play(ReleaseStart::First).await;
    assert!(f.value().await.shuffled);
    assert_eq!(sorted(f.context_order().await), names(&names_in_order));

    let mut moved = false;
    for _ in 0..8 {
        f.play(ReleaseStart::First).await;
        moved |= f.context_order().await != names(&names_in_order);
    }
    assert!(moved, "the lane was shuffled, not left in order");

    assert_eq!(f.play_from("t5").await, "t5", "a play from a track starts with it");
    assert_eq!(f.context_order().await[0], "t5");
    assert_eq!(sorted(f.context_order().await), names(&names_in_order));
    assert!(f.value().await.shuffled);

    f.queue.set_shuffle(false).await.unwrap();
    assert_eq!(
        f.full_order().await,
        names(&["t5"])
            .into_iter()
            .chain({
                let mut rest = names(&names_in_order);
                rest.retain(|name| name != "t5");
                rest
            })
            .collect::<Vec<_>>()
    );
}

/// The mode outlives the lane: clearing what is playing from leaves shuffle
/// as it was, and the next play follows it.
#[tokio::test]
async fn test_clearing_the_context_keeps_the_shuffle_mode() {
    let mut f = release(&["t1", "t2", "t3"]).await;
    f.queue.set_shuffle(true).await.unwrap();
    f.play(ReleaseStart::First).await;

    f.queue.clear_playing_from().await.unwrap();

    let value = f.value().await;
    assert!(value.context.is_none());
    assert!(value.shuffled);
}

/// A list of tracks plays exactly those tracks in the order sent, across
/// releases and with a track named twice, from the entry at the start index:
/// the ones before it are history, the ones after it upcoming.
#[tokio::test]
async fn play_tracks_plays_the_list_in_order_from_the_start_index() {
    let mut f = fixture(&[("r1", &["a1", "a2", "a3"]), ("r2", &["b1", "b2"])]).await;
    let list = f.ids(&["b2", "a1", "b1", "a1", "a3"]);

    assert_eq!(
        f.play_fill(ContextFill::Tracks { track_ids: list, start: TracksStart::Index(3) }).await,
        "a1"
    );

    let value = f.value().await;
    assert_eq!(value.context.unwrap().source, ContextSource::Tracks);
    assert!(value.has_previous, "the entries before the start are behind it");
    assert_eq!(f.context_order().await, names(&["b2", "a1", "b1", "a1", "a3"]));
    assert_eq!(f.upcoming().await, names(&["a3"]));
    assert_eq!(f.previous(0).await, Some("b1".into()), "the start is the second a1, not the first");
}

/// A track the library doesn't hold is left out, the fill names it, and the
/// start still names the entry the list put at its index.
#[tokio::test]
async fn play_tracks_leaves_out_a_track_the_library_lacks() {
    let mut f = fixture(&[("r1", &["a1", "a2", "a3"])]).await;
    let missing = "6f9a1c2e-0b3d-4e5f-8a7b-9c0d1e2f3a4b".to_string();
    let mut list = f.ids(&["a1", "a2", "a3"]);
    list.insert(1, missing.clone());

    let filled = f
        .queue
        .play(ContextFill::Tracks { track_ids: list, start: TracksStart::Index(2) })
        .await
        .unwrap();

    let crate::tables::ContextFilled::Playing { current, left_out, .. } = filled else {
        panic!("the list plays: {filled:?}");
    };
    assert_eq!(f.name(&current.track_id), "a2");
    assert_eq!(left_out, crate::tables::LeftOut::Tracks(vec![missing]));
    assert_eq!(f.context_order().await, names(&["a1", "a2", "a3"]));
    assert_eq!(f.upcoming().await, names(&["a3"]));
}

/// A shuffled list plays every track the library holds in a new order, names
/// the ones it lacks, and turns shuffle on.
#[tokio::test]
async fn play_tracks_shuffled_plays_the_held_tracks_and_turns_shuffle_on() {
    let mut f = fixture(&[("r1", &["a1", "a2", "a3", "a4", "a5", "a6"])]).await;
    let missing = "6f9a1c2e-0b3d-4e5f-8a7b-9c0d1e2f3a4b".to_string();
    let mut list = f.ids(&["a1", "a2", "a3", "a4", "a5", "a6"]);
    list.insert(2, missing.clone());

    let filled =
        f.queue.play(ContextFill::Tracks { track_ids: list, start: TracksStart::Shuffled }).await;

    let crate::tables::ContextFilled::Playing { track_count, left_out, .. } = filled.unwrap()
    else {
        panic!("the list plays");
    };
    assert_eq!(track_count, 6);
    assert_eq!(left_out, crate::tables::LeftOut::Tracks(vec![missing]));
    let value = f.value().await;
    assert!(value.shuffled);
    assert_eq!(value.context.unwrap().source, ContextSource::Tracks);
    let mut played = f.context_order().await;
    played.sort();
    assert_eq!(played, names(&["a1", "a2", "a3", "a4", "a5", "a6"]));
}

/// A shuffled list that is empty, or holds no track the library has, plays
/// nothing and leaves the queue as it was.
#[tokio::test]
async fn play_tracks_shuffled_without_a_held_track_leaves_the_queue() {
    let mut f = release(&["t1", "t2"]).await;
    f.play_from("t1").await;
    let before = f.shown().await;

    let empty = f
        .queue
        .play(ContextFill::Tracks { track_ids: Vec::new(), start: TracksStart::Shuffled })
        .await;
    assert_eq!(empty.unwrap(), crate::tables::ContextFilled::NoTracks);
    let none_held = f
        .queue
        .play(ContextFill::Tracks {
            track_ids: vec!["6f9a1c2e-0b3d-4e5f-8a7b-9c0d1e2f3a4b".to_string()],
            start: TracksStart::Shuffled,
        })
        .await;
    assert_eq!(none_held.unwrap(), crate::tables::ContextFilled::NoTracks);

    assert_eq!(f.shown().await, before);
    assert_eq!(f.current(), "t1");
}

/// A start the list doesn't have, a start track the library doesn't hold, and
/// an empty list each play nothing and leave the queue as it was.
#[tokio::test]
async fn play_tracks_without_a_start_track_leaves_the_queue() {
    let mut f = release(&["t1", "t2"]).await;
    f.play_from("t1").await;
    let before = f.shown().await;

    let past_the_end = f
        .queue
        .play(ContextFill::Tracks { track_ids: f.ids(&["t1", "t2"]), start: TracksStart::Index(2) })
        .await;
    assert_eq!(past_the_end.unwrap(), crate::tables::ContextFilled::StartMissing);
    let not_held = f
        .queue
        .play(ContextFill::Tracks {
            track_ids: vec![f.id("t2"), "6f9a1c2e-0b3d-4e5f-8a7b-9c0d1e2f3a4b".to_string()],
            start: TracksStart::Index(1),
        })
        .await;
    assert_eq!(not_held.unwrap(), crate::tables::ContextFilled::StartMissing);
    let empty = f
        .queue
        .play(ContextFill::Tracks { track_ids: Vec::new(), start: TracksStart::Index(0) })
        .await;
    assert_eq!(empty.unwrap(), crate::tables::ContextFilled::NoTracks);

    assert_eq!(f.shown().await, before);
    assert_eq!(f.current(), "t1");
}
