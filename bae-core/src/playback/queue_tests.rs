//! The service's queue over real queue tables. Tracks are named in the tests
//! (`"t1"`) and stored under UUIDs, which coven requires of a synced row; the
//! fixture maps between the two.

use super::*;
use crate::db::{ContextFill, Database, ReleaseStart};
use crate::playback::ContextSource;
use std::collections::HashMap;

struct Fixture {
    _home: tempfile::TempDir,
    db: Database,
    queue: PlaybackQueue,
    ids: HashMap<String, String>,
    names: HashMap<String, String>,
}

/// A library holding `releases`, each `(release name, track names)` with its
/// tracks in that order, and an empty queue over it.
async fn fixture(releases: &[(&str, &[&str])]) -> Fixture {
    use crate::db::{DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbReleaseMedium, DbTrack};
    let home = tempfile::TempDir::new().unwrap();
    let db = Database::new_test(
        home.path().join("queue.db").to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let artist = DbArtist {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Artist Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: chrono::Utc::now(),
    };
    db.insert_artist(&artist).await.unwrap();
    let mut ids = HashMap::new();
    for (release_name, track_names) in releases {
        let release_id = uuid::Uuid::new_v4().to_string();
        ids.insert(release_name.to_string(), release_id.clone());
        let album = DbAlbum::new_test("Album Title");
        db.insert_album(&album).await.unwrap();
        db.insert_album_artist(&DbAlbumArtist::new_test(&album.id, &artist.id))
            .await
            .unwrap();
        db.insert_release(&DbRelease::new_test(&album.id, &release_id))
            .await
            .unwrap();
        db.insert_release_medium(&DbReleaseMedium::new_test(&release_id))
            .await
            .unwrap();
        for (index, name) in track_names.iter().enumerate() {
            let track_id = uuid::Uuid::new_v4().to_string();
            ids.insert(name.to_string(), track_id.clone());
            let number = u32::try_from(index + 1).unwrap();
            db.insert_track(&DbTrack::new_test(
                &release_id,
                &track_id,
                "Track Title",
                number,
            ))
            .await
            .unwrap();
        }
    }
    let names = ids
        .iter()
        .map(|(name, id)| (id.clone(), name.clone()))
        .collect();
    Fixture {
        _home: home,
        queue: PlaybackQueue::new(db.playback_queue_tables()),
        db,
        ids,
        names,
    }
}

/// One release `r1` holding `tracks`.
async fn release(tracks: &[&str]) -> Fixture {
    fixture(&[("r1", tracks)]).await
}

/// `r1` holding `tracks`, the context lane's, and `r2` holding `manual`, the
/// tracks the tests queue in Up Next.
async fn release_and_manual(tracks: &[&str], manual: &[&str]) -> Fixture {
    fixture(&[("r1", tracks), ("r2", manual)]).await
}

impl Fixture {
    fn id(&self, name: &str) -> String {
        self.ids[name].clone()
    }

    fn ids(&self, names: &[&str]) -> Vec<String> {
        names.iter().map(|name| self.id(name)).collect()
    }

    fn name(&self, id: &str) -> String {
        self.names[id].clone()
    }

    /// Play `r1` from where `start` says.
    async fn play(&mut self, start: ReleaseStart) -> String {
        self.play_fill(ContextFill::Release {
            release_id: self.id("r1"),
            start,
        })
        .await
    }

    /// Play `r1` from the named track.
    async fn play_from(&mut self, track: &str) -> String {
        self.play(ReleaseStart::Track(self.id(track))).await
    }

    async fn play_fill(&mut self, fill: ContextFill) -> String {
        match self.queue.play(fill).await.unwrap() {
            crate::db::ContextFilled::Playing { current, .. } => self.name(&current.track_id),
            other => panic!("the fill plays nothing: {other:?}"),
        }
    }

    async fn add(&mut self, tracks: &[&str]) {
        self.queue.add_to_queue(self.ids(tracks)).await.unwrap();
    }

    async fn value(&self) -> crate::queue::ResolvedQueueSnapshot {
        self.db.queue_value().await.unwrap()
    }

    /// Up Next's entries then the context lane's upcoming ones.
    async fn upcoming_entries(&self) -> Vec<QueueEntry> {
        let value = self.value().await;
        let context = value.context.map(|c| c.upcoming).unwrap_or_default();
        value
            .manual
            .into_iter()
            .chain(context)
            .map(|item| QueueEntry {
                id: item.entry_id.parse().unwrap(),
                track_id: item.track_id,
            })
            .collect()
    }

    /// The flat play order after the current track, by name.
    async fn upcoming(&self) -> Vec<String> {
        self.upcoming_entries()
            .await
            .iter()
            .map(|entry| self.name(&entry.track_id))
            .collect()
    }

    /// The current track then the upcoming ones, by name.
    async fn full_order(&self) -> Vec<String> {
        let mut order = vec![self.current()];
        order.extend(self.upcoming().await);
        order
    }

    fn current(&self) -> String {
        self.name(self.queue.current_track_id().expect("a current track"))
    }

    fn current_name(&self) -> Option<String> {
        self.queue.current_track_id().map(|id| self.name(id))
    }

    /// Every track of the context lane, played or not, by name.
    async fn context_order(&self) -> Vec<String> {
        self.db
            .playback_queue_tables()
            .context_order()
            .await
            .unwrap()
            .iter()
            .map(|id| self.name(id))
            .collect()
    }

    async fn manual_ids(&self) -> Vec<QueueEntryId> {
        self.value()
            .await
            .manual
            .iter()
            .map(|item| item.entry_id.parse().unwrap())
            .collect()
    }

    /// The upcoming entry playing the named track.
    async fn entry(&self, track: &str) -> QueueEntryId {
        let id = self.id(track);
        self.upcoming_entries()
            .await
            .into_iter()
            .find(|entry| entry.track_id == id)
            .expect("the track is upcoming")
            .id
    }

    async fn next(&mut self) -> Option<String> {
        match self.queue.next_entry().await.unwrap() {
            NextEntry::Play(track) | NextEntry::RepeatCurrent(track) => Some(self.name(&track)),
            NextEntry::Stop => None,
        }
    }

    async fn previous(&mut self, at_ms: i64) -> Option<String> {
        match self
            .queue
            .previous_action(TrackTime::from_millis(at_ms))
            .await
            .unwrap()
        {
            PreviousAction::PlayPrevious(track) => Some(self.name(&track)),
            PreviousAction::RestartCurrent => None,
        }
    }

    async fn revision(&self) -> u64 {
        self.value().await.revision
    }
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

fn sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names
}

include!("queue_tests/mutation.rs");
include!("queue_tests/persistence.rs");
include!("queue_tests/projection.rs");
