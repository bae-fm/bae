use std::collections::{HashSet, VecDeque};

use tracing::warn;

use super::RepeatMode;
use crate::id_provider::IdRef;

/// Per-instance identity for an enqueued track. Distinct from `track_id`: the
/// same track enqueued twice yields two entries with two ids, each removable,
/// reorderable, and skippable independently. Minted from the injected
/// `IdProvider` on enqueue.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueueEntryId(pub String);

/// One enqueued instance: a stable per-instance id plus the track it plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueEntry {
    pub id: QueueEntryId,
    pub track_id: String,
}

/// What to do when advancing to the next track
pub enum NextTrack {
    /// Repeat the current track (RepeatMode::Track)
    RepeatCurrent(String),
    /// Play the next track from the queue
    Play(String),
    /// Queue is empty but RepeatMode::Album is set — caller should rebuild the queue
    RepeatAlbumNeeded,
    /// Queue is empty, nothing to play
    Stop,
}

/// What to do when going to the previous track
pub enum PreviousAction {
    /// Go back to the previous track
    PlayPrevious(String),
    /// Restart the current track (past 3s threshold or no previous track)
    RestartCurrent,
}

/// Pure data structure for managing a playback queue.
///
/// Handles queue CRUD and next/previous decision logic without any I/O.
/// Each enqueued track is wrapped in a `QueueEntry` carrying a unique
/// `QueueEntryId`; mutations key on that id, not on a position or `track_id`.
pub struct PlaybackQueue {
    queue: VecDeque<QueueEntry>,
    current_track_id: Option<String>,
    previous_track_id: Option<String>,
    repeat_mode: RepeatMode,
    ids: IdRef,
}

impl PlaybackQueue {
    pub fn new(ids: IdRef) -> Self {
        Self {
            queue: VecDeque::new(),
            current_track_id: None,
            previous_track_id: None,
            repeat_mode: RepeatMode::None,
            ids,
        }
    }

    /// Wrap a track id in a fresh `QueueEntry` with a newly minted id.
    fn mint(&self, track_id: String) -> QueueEntry {
        QueueEntry {
            id: QueueEntryId(self.ids.new_id()),
            track_id,
        }
    }

    /// Locate an entry by its id.
    fn position_of(&self, id: &QueueEntryId) -> Option<usize> {
        self.queue.iter().position(|e| &e.id == id)
    }

    /// Add track IDs to the end of the queue, minting a fresh entry id each.
    pub fn add_to_queue(&mut self, track_ids: Vec<String>) {
        for track_id in track_ids {
            let entry = self.mint(track_id);
            self.queue.push_back(entry);
        }
    }

    /// Add track IDs to the front of the queue (play next), minting ids.
    pub fn add_next(&mut self, track_ids: Vec<String>) {
        for track_id in track_ids.into_iter().rev() {
            let entry = self.mint(track_id);
            self.queue.push_front(entry);
        }
    }

    /// Insert track IDs at a specific position in the queue, minting ids.
    pub fn insert_at(&mut self, index: usize, track_ids: Vec<String>) {
        let pos = index.min(self.queue.len());
        for (i, track_id) in track_ids.into_iter().enumerate() {
            let entry = self.mint(track_id);
            self.queue.insert(pos + i, entry);
        }
    }

    /// Remove the entry with the given id. Returns the removed entry, or logs a
    /// warning and no-ops when the id is not in the queue.
    pub fn remove(&mut self, id: &QueueEntryId) -> Option<QueueEntry> {
        match self.position_of(id) {
            Some(pos) => self.queue.remove(pos),
            None => {
                warn!("remove: queue entry id not found: {}", id.0);
                None
            }
        }
    }

    /// Move the entry `id` to sit immediately before `before`. `before = None`
    /// moves it to the end of the queue. A missing source id, or a missing
    /// `before` target, logs a warning and no-ops.
    pub fn reorder(&mut self, id: &QueueEntryId, before: Option<&QueueEntryId>) {
        let from = match self.position_of(id) {
            Some(pos) => pos,
            None => {
                warn!("reorder: queue entry id not found: {}", id.0);
                return;
            }
        };

        // Resolve the insertion target before removing, so the destination id
        // is validated against the current queue.
        let before_pos = match before {
            Some(before_id) => match self.position_of(before_id) {
                Some(pos) => Some(pos),
                None => {
                    warn!("reorder: before entry id not found: {}", before_id.0);
                    return;
                }
            },
            None => None,
        };

        // `from` came from `position_of`, so it is in bounds and `remove` cannot
        // miss. Reordering an entry to before itself isn't special-cased: it
        // falls through to a remove-and-reinsert at the same position, which is a
        // correct no-op.
        let entry = self
            .queue
            .remove(from)
            .expect("reorder: position_of returned an in-bounds index");

        // After removal, indices past `from` shift down by one.
        let insert_at = match before_pos {
            Some(pos) if pos > from => pos - 1,
            Some(pos) => pos,
            None => self.queue.len(),
        };
        self.queue.insert(insert_at, entry);
    }

    /// Clear the queue.
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// Skip to the entry with the given id: drains every entry ahead of it and
    /// pops it off. Returns the entry to play, or logs a warning and no-ops
    /// when the id is not in the queue.
    pub fn skip_to(&mut self, id: &QueueEntryId) -> Option<QueueEntry> {
        let pos = match self.position_of(id) {
            Some(pos) => pos,
            None => {
                warn!("skip_to: queue entry id not found: {}", id.0);
                return None;
            }
        };

        for _ in 0..pos {
            self.queue.pop_front();
        }

        self.queue.pop_front()
    }

    /// Get the current queue contents as a Vec of entries.
    pub fn entries(&self) -> Vec<QueueEntry> {
        self.queue.iter().cloned().collect()
    }

    /// Set the current track, moving the old current to previous.
    pub fn set_current(&mut self, track_id: String) {
        if let Some(old) = self.current_track_id.take() {
            self.previous_track_id = Some(old);
        }
        self.current_track_id = Some(track_id);
    }

    /// Determine what to do next (called on AutoAdvance or Next).
    /// Does NOT mutate the queue beyond popping the played entry — caller pops
    /// from queue if needed.
    pub fn next_track(&mut self) -> NextTrack {
        if self.repeat_mode == RepeatMode::Track {
            if let Some(ref id) = self.current_track_id {
                return NextTrack::RepeatCurrent(id.clone());
            }
        }

        if let Some(next) = self.queue.pop_front() {
            if let Some(old) = self.current_track_id.take() {
                self.previous_track_id = Some(old);
            }
            NextTrack::Play(next.track_id)
        } else if self.repeat_mode == RepeatMode::Album {
            NextTrack::RepeatAlbumNeeded
        } else {
            NextTrack::Stop
        }
    }

    /// Determine what to do for "previous" action.
    pub fn previous_action(&self, position_ms: u64) -> PreviousAction {
        if position_ms < 3000 {
            if let Some(ref prev_id) = self.previous_track_id {
                return PreviousAction::PlayPrevious(prev_id.clone());
            }
        }
        PreviousAction::RestartCurrent
    }

    pub fn set_repeat_mode(&mut self, mode: RepeatMode) {
        self.repeat_mode = mode;
    }

    pub fn repeat_mode(&self) -> RepeatMode {
        self.repeat_mode
    }

    pub fn current_track_id(&self) -> Option<&str> {
        self.current_track_id.as_deref()
    }

    pub fn previous_track_id(&self) -> Option<&str> {
        self.previous_track_id.as_deref()
    }

    pub fn set_previous_track_id(&mut self, track_id: Option<String>) {
        self.previous_track_id = track_id;
    }

    /// Replace the entire queue contents with freshly minted entries for the
    /// given track ids.
    pub fn replace(&mut self, track_ids: VecDeque<String>) {
        self.queue = track_ids.into_iter().map(|t| self.mint(t)).collect();
    }

    /// Pop the front entry's track id off the queue.
    pub fn pop_front(&mut self) -> Option<String> {
        self.queue.pop_front().map(|e| e.track_id)
    }

    /// Peek at the front entry's track id.
    pub fn front(&self) -> Option<&str> {
        self.queue.front().map(|e| e.track_id.as_str())
    }

    /// Number of tracks in the queue.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Remove all entries whose track id is in the given set (a deleted track is
    /// removed everywhere it sits in the queue). Also clears `current_track_id`
    /// and `previous_track_id` if they match.
    pub fn remove_by_ids(&mut self, ids: &HashSet<String>) {
        self.queue.retain(|entry| !ids.contains(&entry.track_id));
        if let Some(ref current) = self.current_track_id {
            if ids.contains(current) {
                self.current_track_id = None;
            }
        }
        if let Some(ref prev) = self.previous_track_id {
            if ids.contains(prev) {
                self.previous_track_id = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id_provider::SequentialIdProvider;
    use std::sync::Arc;

    fn queue() -> PlaybackQueue {
        PlaybackQueue::new(Arc::new(SequentialIdProvider::new("entry")))
    }

    /// Collect the queue's track ids in order — the per-instance ids vary, so
    /// most behavioral assertions are about which tracks sit where.
    fn track_ids(q: &PlaybackQueue) -> Vec<String> {
        q.entries().into_iter().map(|e| e.track_id).collect()
    }

    #[test]
    fn test_add_to_queue() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(track_ids(&q), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_add_to_queue_mints_distinct_ids() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "a".into()]);
        let entries = q.entries();
        assert_eq!(entries.len(), 2);
        assert_ne!(
            entries[0].id, entries[1].id,
            "duplicate tracks get distinct ids"
        );
        assert_eq!(entries[0].track_id, entries[1].track_id);
    }

    #[test]
    fn test_add_next_preserves_order() {
        let mut q = queue();
        q.add_to_queue(vec!["x".into()]);
        q.add_next(vec!["a".into(), "b".into()]);
        assert_eq!(track_ids(&q), vec!["a", "b", "x"]);
    }

    #[test]
    fn test_remove_by_entry_id() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into()]);
        let b_id = q.entries()[1].id.clone();
        let removed = q.remove(&b_id);
        assert_eq!(removed.map(|e| e.track_id), Some("b".into()));
        assert_eq!(track_ids(&q), vec!["a", "c"]);
    }

    /// The load-bearing dup test: the same track enqueued twice, removing one
    /// instance by its id leaves the other instance — and its id — intact.
    #[test]
    fn test_remove_one_duplicate_keeps_the_other() {
        let mut q = queue();
        q.add_to_queue(vec!["dup".into(), "dup".into()]);
        let entries = q.entries();
        let first_id = entries[0].id.clone();
        let second_id = entries[1].id.clone();

        let removed = q.remove(&first_id).expect("first instance removed");
        assert_eq!(removed.id, first_id);
        assert_eq!(removed.track_id, "dup");

        let remaining = q.entries();
        assert_eq!(remaining.len(), 1, "exactly one instance remains");
        assert_eq!(
            remaining[0].id, second_id,
            "the other instance's id survives"
        );
        assert_eq!(remaining[0].track_id, "dup");
    }

    #[test]
    fn test_remove_unknown_id_is_noop() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into()]);
        assert_eq!(q.remove(&QueueEntryId("nope".into())), None);
        assert_eq!(track_ids(&q), vec!["a"]);
    }

    #[test]
    fn test_reorder_forward() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into(), "d".into()]);
        let ids: Vec<_> = q.entries().into_iter().map(|e| e.id).collect();
        // Move "a" before "c": expect b, a, c, d.
        q.reorder(&ids[0], Some(&ids[2]));
        assert_eq!(track_ids(&q), vec!["b", "a", "c", "d"]);
    }

    #[test]
    fn test_reorder_to_end() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into(), "d".into()]);
        let ids: Vec<_> = q.entries().into_iter().map(|e| e.id).collect();
        // Move "a" to the end (before = None).
        q.reorder(&ids[0], None);
        assert_eq!(track_ids(&q), vec!["b", "c", "d", "a"]);
    }

    #[test]
    fn test_reorder_backward() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into(), "d".into()]);
        let ids: Vec<_> = q.entries().into_iter().map(|e| e.id).collect();
        // Move "c" before "a": expect c, a, b, d.
        q.reorder(&ids[2], Some(&ids[0]));
        assert_eq!(track_ids(&q), vec!["c", "a", "b", "d"]);
    }

    #[test]
    fn test_reorder_before_self_is_noop() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into()]);
        let ids: Vec<_> = q.entries().into_iter().map(|e| e.id).collect();
        q.reorder(&ids[1], Some(&ids[1]));
        assert_eq!(track_ids(&q), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_reorder_unknown_source_is_noop() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "b".into()]);
        q.reorder(&QueueEntryId("nope".into()), None);
        assert_eq!(track_ids(&q), vec!["a", "b"]);
    }

    #[test]
    fn test_reorder_unknown_before_is_noop() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "b".into()]);
        let a_id = q.entries()[0].id.clone();
        q.reorder(&a_id, Some(&QueueEntryId("nope".into())));
        assert_eq!(track_ids(&q), vec!["a", "b"]);
    }

    #[test]
    fn test_clear() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "b".into()]);
        q.clear();
        assert!(q.entries().is_empty());
    }

    #[test]
    fn test_skip_to_by_entry_id() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into(), "d".into()]);
        let c_id = q.entries()[2].id.clone();
        let entry = q.skip_to(&c_id);
        assert_eq!(entry.map(|e| e.track_id), Some("c".into()));
        assert_eq!(track_ids(&q), vec!["d"]);
    }

    #[test]
    fn test_skip_to_unknown_id_is_noop() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into()]);
        assert_eq!(q.skip_to(&QueueEntryId("nope".into())), None);
        assert_eq!(track_ids(&q), vec!["a"]);
    }

    #[test]
    fn test_set_current_moves_to_previous() {
        let mut q = queue();
        q.set_current("track1".into());
        assert_eq!(q.current_track_id(), Some("track1"));
        assert_eq!(q.previous_track_id(), None);

        q.set_current("track2".into());
        assert_eq!(q.current_track_id(), Some("track2"));
        assert_eq!(q.previous_track_id(), Some("track1"));
    }

    #[test]
    fn test_next_track_from_queue() {
        let mut q = queue();
        q.set_current("current".into());
        q.add_to_queue(vec!["next1".into(), "next2".into()]);
        match q.next_track() {
            NextTrack::Play(id) => assert_eq!(id, "next1"),
            _ => panic!("Expected Play"),
        }
        assert_eq!(q.previous_track_id(), Some("current"));
        assert_eq!(track_ids(&q), vec!["next2"]);
    }

    #[test]
    fn test_next_track_repeat_current() {
        let mut q = queue();
        q.set_current("track1".into());
        q.set_repeat_mode(RepeatMode::Track);
        match q.next_track() {
            NextTrack::RepeatCurrent(id) => assert_eq!(id, "track1"),
            _ => panic!("Expected RepeatCurrent"),
        }
    }

    #[test]
    fn test_next_track_repeat_album_needed() {
        let mut q = queue();
        q.set_repeat_mode(RepeatMode::Album);
        match q.next_track() {
            NextTrack::RepeatAlbumNeeded => {}
            _ => panic!("Expected RepeatAlbumNeeded"),
        }
    }

    #[test]
    fn test_previous_action_restart_when_past_3s() {
        let q = queue();
        match q.previous_action(5000) {
            PreviousAction::RestartCurrent => {}
            _ => panic!("Expected RestartCurrent"),
        }
    }

    #[test]
    fn test_previous_action_go_back() {
        let mut q = queue();
        q.set_current("track1".into());
        q.set_current("track2".into());
        match q.previous_action(1000) {
            PreviousAction::PlayPrevious(id) => assert_eq!(id, "track1"),
            _ => panic!("Expected PlayPrevious"),
        }
    }

    #[test]
    fn test_previous_action_restart_when_no_previous() {
        let mut q = queue();
        q.set_current("track1".into());
        match q.previous_action(1000) {
            PreviousAction::RestartCurrent => {}
            _ => panic!("Expected RestartCurrent"),
        }
    }

    #[test]
    fn test_repeat_mode_default() {
        let q = queue();
        assert_eq!(q.repeat_mode(), RepeatMode::None);
    }

    #[test]
    fn test_insert_at_middle() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into()]);
        q.insert_at(1, vec!["x".into(), "y".into()]);
        assert_eq!(track_ids(&q), vec!["a", "x", "y", "b", "c"]);
    }

    #[test]
    fn test_insert_at_beginning() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "b".into()]);
        q.insert_at(0, vec!["x".into()]);
        assert_eq!(track_ids(&q), vec!["x", "a", "b"]);
    }

    #[test]
    fn test_insert_at_end() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "b".into()]);
        q.insert_at(2, vec!["x".into()]);
        assert_eq!(track_ids(&q), vec!["a", "b", "x"]);
    }

    #[test]
    fn test_insert_at_beyond_end_clamps() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into()]);
        q.insert_at(999, vec!["x".into()]);
        assert_eq!(track_ids(&q), vec!["a", "x"]);
    }

    #[test]
    fn test_replace() {
        let mut q = queue();
        q.add_to_queue(vec!["old".into()]);
        let mut new_queue = VecDeque::new();
        new_queue.push_back("new1".into());
        new_queue.push_back("new2".into());
        q.replace(new_queue);
        assert_eq!(track_ids(&q), vec!["new1", "new2"]);
    }

    #[test]
    fn test_remove_by_ids_removes_matching_tracks() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into(), "d".into()]);
        let ids: HashSet<String> = ["b", "d"].iter().map(|s| s.to_string()).collect();
        q.remove_by_ids(&ids);
        assert_eq!(track_ids(&q), vec!["a", "c"]);
    }

    #[test]
    fn test_remove_by_ids_removes_every_instance_of_a_track() {
        let mut q = queue();
        q.add_to_queue(vec!["a".into(), "dup".into(), "b".into(), "dup".into()]);
        let ids: HashSet<String> = ["dup"].iter().map(|s| s.to_string()).collect();
        q.remove_by_ids(&ids);
        assert_eq!(track_ids(&q), vec!["a", "b"]);
    }

    #[test]
    fn test_remove_by_ids_clears_current_track() {
        let mut q = queue();
        q.set_current("track1".into());
        q.add_to_queue(vec!["a".into()]);
        let ids: HashSet<String> = ["track1"].iter().map(|s| s.to_string()).collect();
        q.remove_by_ids(&ids);
        assert_eq!(q.current_track_id(), None);
        assert_eq!(track_ids(&q), vec!["a"]);
    }

    #[test]
    fn test_remove_by_ids_clears_previous_track() {
        let mut q = queue();
        q.set_current("track1".into());
        q.set_current("track2".into());
        assert_eq!(q.previous_track_id(), Some("track1"));
        let ids: HashSet<String> = ["track1"].iter().map(|s| s.to_string()).collect();
        q.remove_by_ids(&ids);
        assert_eq!(q.previous_track_id(), None);
        assert_eq!(q.current_track_id(), Some("track2"));
    }

    #[test]
    fn test_remove_by_ids_no_match_is_noop() {
        let mut q = queue();
        q.set_current("current".into());
        q.add_to_queue(vec!["a".into(), "b".into()]);
        let ids: HashSet<String> = ["x", "y"].iter().map(|s| s.to_string()).collect();
        q.remove_by_ids(&ids);
        assert_eq!(track_ids(&q), vec!["a", "b"]);
        assert_eq!(q.current_track_id(), Some("current"));
    }

    #[test]
    fn test_remove_by_ids_removes_all() {
        let mut q = queue();
        q.set_current("current".into());
        q.add_to_queue(vec!["a".into(), "b".into()]);
        let ids: HashSet<String> = ["a", "b", "current"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        q.remove_by_ids(&ids);
        assert!(q.entries().is_empty());
        assert_eq!(q.current_track_id(), None);
    }

    #[test]
    fn test_next_track_repeat_track_without_current_falls_through() {
        // RepeatMode::Track only repeats when there IS a current track. With
        // none set, it must fall through to the normal advance.
        let mut q = queue();
        q.set_repeat_mode(RepeatMode::Track);
        q.add_to_queue(vec!["a".into(), "b".into()]);
        match q.next_track() {
            NextTrack::Play(id) => assert_eq!(id, "a"),
            _ => panic!("expected Play"),
        }
        // And with an empty queue (still no current) it stops rather than repeating.
        let mut q = queue();
        q.set_repeat_mode(RepeatMode::Track);
        assert!(matches!(q.next_track(), NextTrack::Stop));
    }

    #[test]
    fn test_front_peeks_without_removing() {
        let mut q = queue();
        assert_eq!(q.front(), None);
        q.add_to_queue(vec!["a".into(), "b".into()]);
        assert_eq!(q.front(), Some("a"));
        assert_eq!(q.len(), 2, "front must not consume the queue");
    }

    #[test]
    fn test_pop_front_empty_returns_none() {
        let mut q = queue();
        assert_eq!(q.pop_front(), None);
    }
}
