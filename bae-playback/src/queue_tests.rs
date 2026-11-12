//! The service's queue over real queue tables. Tracks are named in the tests
//! (`"t1"`) and stored under UUIDs, which coven requires of a synced row; the
//! fixture maps between the two.

use super::*;
use crate::tables::{ContextFill, ReleaseStart};
use crate::ContextSource;
use bae_store::Store;
use std::collections::HashMap;

pub(super) struct Fixture {
    pub(super) _home: tempfile::TempDir,
    pub(super) db: Store,
    pub(super) queue: PlaybackQueue,
    pub(super) ids: HashMap<String, String>,
    pub(super) names: HashMap<String, String>,
}

/// A library holding `releases`, each `(release name, track names)` with its
/// tracks in that order, and an empty queue over it.
pub(super) async fn fixture(releases: &[(&str, &[&str])]) -> Fixture {
    use bae_records::{DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbReleaseMedium, DbTrack};
    let home = tempfile::TempDir::new().unwrap();
    let db =
        Store::new_test(crate::TEST_SCHEMA, home.path(), std::sync::Arc::new(coven::SystemClock))
            .await
            .unwrap();
    let records = bae_records::test_fixtures::test_records(&db);
    let artist = DbArtist {
        id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
        name: "Artist Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: chrono::Utc::now(),
    };
    records.insert_artist(&artist).await.unwrap();
    let mut ids = HashMap::new();
    for (release_name, track_names) in releases {
        let release_id = coven::IdSource::new_id(&coven::UuidIds).to_string();
        ids.insert(release_name.to_string(), release_id.clone());
        let album = DbAlbum::new_test("Album Title", &coven::UuidIds);
        records.insert_album(&album).await.unwrap();
        records.insert_album_artist(&DbAlbumArtist::new_test(&album.id, &artist.id)).await.unwrap();
        records.insert_release(&DbRelease::new_test(&album.id, &release_id)).await.unwrap();
        records.insert_release_medium(&DbReleaseMedium::new_test(&release_id)).await.unwrap();
        for (index, name) in track_names.iter().enumerate() {
            let track_id = coven::IdSource::new_id(&coven::UuidIds).to_string();
            ids.insert(name.to_string(), track_id.clone());
            let number = u32::try_from(index + 1).unwrap();
            records
                .insert_track(&DbTrack::new_test(&release_id, &track_id, "Track Title", number))
                .await
                .unwrap();
        }
    }
    let names = ids.iter().map(|(name, id)| (id.clone(), name.clone())).collect();
    Fixture {
        _home: home,
        queue: PlaybackQueue::new(crate::tables::PlaybackQueueTables::new(db.clone())),
        db,
        ids,
        names,
    }
}

/// One release `r1` holding `tracks`.
pub(super) async fn release(tracks: &[&str]) -> Fixture {
    fixture(&[("r1", tracks)]).await
}

/// `r1` holding `tracks`, the context lane's, and `r2` holding `manual`, the
/// tracks the tests queue in Up Next.
pub(super) async fn release_and_manual(tracks: &[&str], manual: &[&str]) -> Fixture {
    fixture(&[("r1", tracks), ("r2", manual)]).await
}

impl Fixture {
    pub(super) fn id(&self, name: &str) -> String {
        self.ids[name].clone()
    }

    pub(super) fn ids(&self, names: &[&str]) -> Vec<String> {
        names.iter().map(|name| self.id(name)).collect()
    }

    pub(super) fn name(&self, id: &str) -> String {
        self.names[id].clone()
    }

    /// Play `r1` from where `start` says.
    pub(super) async fn play(&mut self, start: ReleaseStart) -> String {
        self.play_fill(ContextFill::Release { release_id: self.id("r1"), start }).await
    }

    /// Play `r1` from the named track.
    pub(super) async fn play_from(&mut self, track: &str) -> String {
        self.play(ReleaseStart::Track(self.id(track))).await
    }

    pub(super) async fn play_fill(&mut self, fill: ContextFill) -> String {
        match self.queue.play(fill).await.unwrap() {
            crate::tables::ContextFilled::Playing { current, .. } => self.name(&current.track_id),
            other => panic!("the fill plays nothing: {other:?}"),
        }
    }

    pub(super) async fn add(&mut self, tracks: &[&str]) {
        self.queue.add_to_queue(self.ids(tracks)).await.unwrap();
    }

    pub(super) async fn value(&self) -> crate::ResolvedQueueSnapshot {
        crate::tables::queue_value(&self.db).await.unwrap()
    }

    /// Up Next's entries then the context lane's upcoming ones.
    pub(super) async fn upcoming_entries(&self) -> Vec<QueueEntry> {
        let value = self.value().await;
        let context = crate::tables::queue_upcoming(&self.db).await.unwrap();
        value
            .manual
            .into_iter()
            .chain(context)
            .map(|item| QueueEntry { id: item.entry_id.parse().unwrap(), track_id: item.track_id })
            .collect()
    }

    /// The flat play order after the current track, by name.
    pub(super) async fn upcoming(&self) -> Vec<String> {
        self.upcoming_entries().await.iter().map(|entry| self.name(&entry.track_id)).collect()
    }

    /// The current track then the upcoming ones, by name.
    pub(super) async fn full_order(&self) -> Vec<String> {
        let mut order = vec![self.current()];
        order.extend(self.upcoming().await);
        order
    }

    pub(super) fn current(&self) -> String {
        self.name(self.queue.current_track_id().expect("a current track"))
    }

    pub(super) fn current_name(&self) -> Option<String> {
        self.queue.current_track_id().map(|id| self.name(id))
    }

    /// Every track of the context lane, played or not, by name.
    pub(super) async fn context_order(&self) -> Vec<String> {
        crate::tables::PlaybackQueueTables::new(self.db.clone())
            .context_order()
            .await
            .unwrap()
            .iter()
            .map(|id| self.name(id))
            .collect()
    }

    pub(super) async fn manual_ids(&self) -> Vec<QueueEntryId> {
        self.value().await.manual.iter().map(|item| item.entry_id.parse().unwrap()).collect()
    }

    /// The first upcoming entry playing the named track.
    pub(super) async fn entry(&self, track: &str) -> QueueEntryId {
        let id = self.id(track);
        self.upcoming_entries()
            .await
            .into_iter()
            .find(|entry| entry.track_id == id)
            .expect("the track is upcoming")
            .id
    }

    pub(super) async fn next(&mut self) -> Option<String> {
        match self.queue.next_entry().await.unwrap() {
            NextEntry::Play(track) | NextEntry::RepeatCurrent(track) => Some(self.name(&track)),
            NextEntry::Stop => None,
        }
    }

    pub(super) async fn previous(&mut self, at_ms: i64) -> Option<String> {
        match self.queue.previous_action(TrackTime::from_millis(at_ms)).await.unwrap() {
            PreviousAction::PlayPrevious(track) => Some(self.name(&track)),
            PreviousAction::RestartCurrent => None,
        }
    }

    /// Everything the queue screens and the tables show, to tell that an
    /// operation changed nothing.
    pub(super) async fn shown(&self) -> (crate::ResolvedQueueSnapshot, Vec<String>, Vec<String>) {
        (self.value().await, self.upcoming().await, self.context_order_or_empty().await)
    }

    pub(super) async fn context_order_or_empty(&self) -> Vec<String> {
        self.context_order().await
    }
}

pub(super) fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

pub(super) fn sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names
}

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
    assert_ne!(ids[0].clone(), ids[1].clone(), "duplicate tracks get distinct ids");
}

/// A track the library does not hold is left out, and the count says so.
#[tokio::test]
async fn test_add_to_queue_leaves_out_tracks_the_library_lacks() {
    let mut f = release(&["a"]).await;
    let added = f
        .queue
        .add_to_queue(vec![f.id("a"), coven::IdSource::new_id(&coven::UuidIds).to_string()])
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
    let b_id = f.manual_ids().await[1].clone();
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
    let removed = f.queue.remove(ids[0].clone()).await.unwrap().expect("first instance removed");
    assert_eq!(removed.id, ids[0].clone());
    assert_eq!(f.manual_ids().await, vec![ids[1].clone()], "the other instance's id survives");
}

#[tokio::test]
async fn test_remove_unknown_id_is_noop() {
    let mut f = release(&["a"]).await;
    f.add(&["a"]).await;
    let before = f.shown().await;
    assert_eq!(f.queue.remove(QueueEntryId::Manual(i64::MAX)).await.unwrap(), None);
    assert_eq!(f.upcoming().await, names(&["a"]));
    assert_eq!(f.shown().await, before, "an unknown id changes nothing");
}

#[tokio::test]
async fn test_reorder_forward() {
    let mut f = release(&["a", "b", "c", "d"]).await;
    f.add(&["a", "b", "c", "d"]).await;
    let ids = f.manual_ids().await;
    assert!(f.queue.reorder(ids[0].clone(), Some(ids[2].clone())).await.unwrap().is_some());
    assert_eq!(f.upcoming().await, names(&["b", "a", "c", "d"]));
}

#[tokio::test]
async fn test_reorder_to_end() {
    let mut f = release(&["a", "b", "c", "d"]).await;
    f.add(&["a", "b", "c", "d"]).await;
    let ids = f.manual_ids().await;
    f.queue.reorder(ids[0].clone(), None).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["b", "c", "d", "a"]));
}

#[tokio::test]
async fn test_reorder_backward() {
    let mut f = release(&["a", "b", "c", "d"]).await;
    f.add(&["a", "b", "c", "d"]).await;
    let ids = f.manual_ids().await;
    f.queue.reorder(ids[2].clone(), Some(ids[0].clone())).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["c", "a", "b", "d"]));
}

/// A reorder over a lane with gaps — entries removed from between — keeps
/// every survivor's place.
#[tokio::test]
async fn test_reorder_across_gaps() {
    let mut f = release(&["a", "b", "c", "d", "e"]).await;
    f.add(&["a", "b", "c", "d", "e"]).await;
    let ids = f.manual_ids().await;
    f.queue.remove(ids[1].clone()).await.unwrap();
    f.queue.remove(ids[3].clone()).await.unwrap();
    f.queue.reorder(ids[0].clone(), Some(ids[4].clone())).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["c", "a", "e"]));
    f.queue.reorder(ids[4].clone(), Some(ids[2].clone())).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["e", "c", "a"]));
}

#[tokio::test]
async fn test_reorder_before_self_is_noop() {
    let mut f = release(&["a", "b", "c"]).await;
    f.add(&["a", "b", "c"]).await;
    let ids = f.manual_ids().await;
    f.queue.reorder(ids[1].clone(), Some(ids[1].clone())).await.unwrap();
    assert_eq!(f.upcoming().await, names(&["a", "b", "c"]));
}

#[tokio::test]
async fn test_reorder_unknown_source_is_noop() {
    let mut f = release(&["a", "b"]).await;
    f.add(&["a", "b"]).await;
    assert!(f.queue.reorder(QueueEntryId::Manual(i64::MAX), None).await.unwrap().is_none());
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

#[tokio::test]
async fn test_skip_to_manual_drains_prefix() {
    let mut f = release(&["a", "b", "c", "d"]).await;
    f.add(&["a", "b", "c", "d"]).await;
    let c_id = f.manual_ids().await[2].clone();
    let entry = f.queue.skip_to(c_id).await.unwrap();
    assert_eq!(entry.map(|e| f.name(&e.track_id)), Some("c".into()));
    assert_eq!(f.upcoming().await, names(&["d"]));
    assert_eq!(f.current(), "c");
}

#[tokio::test]
async fn test_skip_to_unknown_id_is_noop() {
    let mut f = release(&["a"]).await;
    f.add(&["a"]).await;
    assert_eq!(f.queue.skip_to(QueueEntryId::Manual(i64::MAX)).await.unwrap(), None);
    assert_eq!(f.upcoming().await, names(&["a"]));
}

/// Removing one of a track's two context entries by its id leaves the other,
/// with its id, where it was.
#[tokio::test]
async fn test_remove_one_context_duplicate_keeps_the_other() {
    let mut f = release(&["a1", "a2"]).await;
    f.play_fill(ContextFill::Releases(vec![f.id("r1"), f.id("r1")])).await;
    let upcoming = f.upcoming_entries().await;
    assert_eq!(f.upcoming().await, names(&["a2", "a1", "a2"]));
    let removed =
        f.queue.remove(upcoming[0].id.clone()).await.unwrap().expect("the first a2 removed");
    assert_eq!(removed, upcoming[0]);
    assert_eq!(f.upcoming_entries().await, upcoming[1..].to_vec());
}

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

    let mut restored = PlaybackQueue::new(crate::tables::PlaybackQueueTables::new(f.db.clone()));
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
    assert!(f.queue.reorder(manual, Some(context)).await.unwrap().is_none());
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
        crate::tables::queue_upcoming(&f.db)
            .await
            .unwrap()
            .iter()
            .map(|i| f.name(&i.track_id))
            .collect::<Vec<_>>(),
        names(&["t2", "t3"])
    );
    assert!(!value.shuffled);
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

    let windows = bae_store::LibraryPageWindows::from([
        bae_store::LibraryPageWindow { offset: 0, limit: 3 },
        bae_store::LibraryPageWindow { offset: 100, limit: 20 },
        bae_store::LibraryPageWindow { offset: 125, limit: 20 },
    ]);
    let mut live = crate::tables::subscribe_queue_upcoming(&f.db, windows);
    let read = live.next().await.expect("the query stays open").result.unwrap();
    let window_tracks: Vec<Vec<String>> =
        read.windows.iter().map(|w| w.rows.iter().map(|i| f.name(&i.track_id)).collect()).collect();
    assert_eq!(window_tracks[0], tracks[1..4].to_vec());
    assert_eq!(window_tracks[1], tracks[101..121].to_vec());
    assert_eq!(
        window_tracks[2],
        tracks[126..130].to_vec(),
        "a window past the end holds what remains"
    );
    assert_eq!(read.upcoming_total, 129);
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

/// The queue screens follow a play's fill, Clear Up Next and Clear playing
/// from, which are bulk writes, as they follow every other change: each
/// reaches the live queries reading the lanes.
#[tokio::test]
async fn test_the_queue_screens_follow_a_play_and_the_clears() {
    let mut f = release_and_manual(&["t1", "t2", "t3"], &["m1"]).await;
    let mut value = crate::tables::subscribe_queue_value(&f.db);
    let mut upcoming = crate::tables::subscribe_queue_upcoming(
        &f.db,
        bae_store::LibraryPageWindows::from([bae_store::LibraryPageWindow {
            offset: 0,
            limit: 10,
        }]),
    );
    macro_rules! value_until {
        ($check:expr) => {
            tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
                loop {
                    let read = value.next().await.expect("the query stays open").unwrap();
                    if $check(&read) {
                        break;
                    }
                }
            })
            .await
            .expect("the queue value follows the write")
        };
    }
    macro_rules! upcoming_until {
        ($total:expr) => {
            tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
                loop {
                    let read = upcoming.next().await.expect("the query stays open").result.unwrap();
                    if read.upcoming_total == $total {
                        break;
                    }
                }
            })
            .await
            .expect("the upcoming windows follow the write")
        };
    }
    upcoming_until!(0);

    f.play_from("t1").await;
    upcoming_until!(2);
    f.add(&["m1"]).await;
    value_until!(|read: &crate::ResolvedQueueSnapshot| read.manual.len() == 1);

    f.queue.clear_up_next().await.unwrap();
    value_until!(
        |read: &crate::ResolvedQueueSnapshot| read.manual.is_empty() && read.context.is_some()
    );

    f.queue.clear_playing_from().await.unwrap();
    upcoming_until!(0);
    value_until!(|read: &crate::ResolvedQueueSnapshot| read.context.is_none());
}

/// Up Next's version and the context lane's, as the queue screens read them.
async fn versions(f: &Fixture) -> (u64, u64) {
    (f.value().await.manual_version, crate::tables::queue_context_version(&f.db).await.unwrap())
}

/// Each lane's version counts the writes that change that lane, and only
/// those: Up Next's entries put in, taken out or moved; the context lane's
/// tracks after the playing one put in, taken out or moved, or which one
/// plays. A reorder answers the version its move made.
#[tokio::test]
async fn each_lanes_version_counts_the_writes_that_change_it() {
    let mut f = release_and_manual(&["t1", "t2", "t3", "t4"], &["m1", "m2", "m3"]).await;
    assert_eq!(versions(&f).await, (0, 0));

    f.play_from("t1").await;
    assert_eq!(versions(&f).await, (0, 1), "a play fills the context lane");
    f.add(&["m1", "m2", "m3"]).await;
    assert_eq!(versions(&f).await, (1, 1), "an addition changes Up Next alone");

    let manual = f.manual_ids().await;
    let moved = f.queue.reorder(manual[2].clone(), Some(manual[0].clone())).await.unwrap();
    assert_eq!(moved, Some(2), "a reorder answers the version it made");
    assert_eq!(versions(&f).await, (2, 1));

    let (t2, t4) = (f.entry("t2").await, f.entry("t4").await);
    let moved = f.queue.reorder(t4, Some(t2)).await.unwrap();
    assert_eq!(moved, Some(2), "a context move counts on the context lane");
    assert_eq!(versions(&f).await, (2, 2));

    assert_eq!(
        f.queue.reorder(QueueEntryId::Manual(i64::MAX), None).await.unwrap(),
        None,
        "a refused reorder moves nothing"
    );
    assert_eq!(versions(&f).await, (2, 2), "and counts nothing");

    f.next().await;
    assert_eq!(versions(&f).await, (3, 2), "playing on takes Up Next's first entry");
    f.queue.clear_up_next().await.unwrap();
    assert_eq!(versions(&f).await, (4, 2));
    f.queue.clear_up_next().await.unwrap();
    assert_eq!(versions(&f).await, (4, 2), "clearing an empty Up Next changes nothing");
    f.next().await;
    assert_eq!(versions(&f).await, (4, 3), "playing on in the context lane moves its cursor");
    f.queue.set_repeat_mode(RepeatMode::Off).await.unwrap();
    assert_eq!(versions(&f).await, (4, 3), "the repeat mode is no lane's");
}
