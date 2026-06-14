use std::collections::{HashSet, VecDeque};

use super::RepeatMode;

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
pub struct PlaybackQueue {
    queue: VecDeque<String>,
    current_track_id: Option<String>,
    previous_track_id: Option<String>,
    repeat_mode: RepeatMode,
}

impl Default for PlaybackQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaybackQueue {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            current_track_id: None,
            previous_track_id: None,
            repeat_mode: RepeatMode::None,
        }
    }

    /// Add track IDs to the end of the queue.
    pub fn add_to_queue(&mut self, track_ids: Vec<String>) {
        for track_id in track_ids {
            self.queue.push_back(track_id);
        }
    }

    /// Add track IDs to the front of the queue (play next).
    pub fn add_next(&mut self, track_ids: Vec<String>) {
        for track_id in track_ids.into_iter().rev() {
            self.queue.push_front(track_id);
        }
    }

    /// Insert track IDs at a specific position in the queue.
    pub fn insert_at(&mut self, index: usize, track_ids: Vec<String>) {
        let pos = index.min(self.queue.len());
        for (i, track_id) in track_ids.into_iter().enumerate() {
            self.queue.insert(pos + i, track_id);
        }
    }

    /// Remove a track at the given index. Returns the removed track ID if valid.
    pub fn remove(&mut self, index: usize) -> Option<String> {
        if index < self.queue.len() {
            self.queue.remove(index)
        } else {
            None
        }
    }

    /// Reorder: move track from `from` index to `to` index.
    /// `to` may equal `queue.len()` to move an item to the end.
    pub fn reorder(&mut self, from: usize, to: usize) {
        if from < self.queue.len() && to <= self.queue.len() && from != to {
            if let Some(track_id) = self.queue.remove(from) {
                if to > from {
                    self.queue.insert(to - 1, track_id);
                } else {
                    self.queue.insert(to, track_id);
                }
            }
        }
    }

    /// Clear the queue.
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// Skip to a specific position in the queue.
    /// Drains all tracks before the target index and pops the target.
    /// Returns the track ID to play, or None if index is out of bounds.
    pub fn skip_to(&mut self, index: usize) -> Option<String> {
        if index >= self.queue.len() {
            return None;
        }

        for _ in 0..index {
            self.queue.pop_front();
        }

        self.queue.pop_front()
    }

    /// Get the current queue contents as a Vec of track IDs.
    pub fn tracks(&self) -> Vec<String> {
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
    /// Does NOT mutate the queue — caller pops from queue if needed.
    pub fn next_track(&mut self) -> NextTrack {
        if self.repeat_mode == RepeatMode::Track {
            if let Some(ref id) = self.current_track_id {
                return NextTrack::RepeatCurrent(id.clone());
            }
        }

        if let Some(next_id) = self.queue.pop_front() {
            if let Some(old) = self.current_track_id.take() {
                self.previous_track_id = Some(old);
            }
            NextTrack::Play(next_id)
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

    /// Replace the entire queue contents.
    pub fn replace(&mut self, queue: VecDeque<String>) {
        self.queue = queue;
    }

    /// Pop from the front of the queue.
    pub fn pop_front(&mut self) -> Option<String> {
        self.queue.pop_front()
    }

    /// Peek at the front of the queue.
    pub fn front(&self) -> Option<&String> {
        self.queue.front()
    }

    /// Number of tracks in the queue.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Remove all tracks whose IDs are in the given set.
    /// Also clears `current_track_id` and `previous_track_id` if they match.
    pub fn remove_by_ids(&mut self, ids: &HashSet<String>) {
        self.queue.retain(|id| !ids.contains(id));
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

    #[test]
    fn test_add_to_queue() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(q.tracks(), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_add_next_preserves_order() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["x".into()]);
        q.add_next(vec!["a".into(), "b".into()]);
        assert_eq!(q.tracks(), vec!["a", "b", "x"]);
    }

    #[test]
    fn test_remove() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into()]);
        let removed = q.remove(1);
        assert_eq!(removed, Some("b".into()));
        assert_eq!(q.tracks(), vec!["a", "c"]);
    }

    #[test]
    fn test_remove_out_of_bounds() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into()]);
        assert_eq!(q.remove(5), None);
        assert_eq!(q.tracks(), vec!["a"]);
    }

    #[test]
    fn test_reorder_forward() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into(), "d".into()]);
        q.reorder(0, 2);
        assert_eq!(q.tracks(), vec!["b", "a", "c", "d"]);
    }

    #[test]
    fn test_reorder_forward_to_end() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into(), "d".into()]);
        q.reorder(0, 4);
        assert_eq!(q.tracks(), vec!["b", "c", "d", "a"]);
    }

    #[test]
    fn test_reorder_backward() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into(), "d".into()]);
        q.reorder(2, 0);
        assert_eq!(q.tracks(), vec!["c", "a", "b", "d"]);
    }

    #[test]
    fn test_clear() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into(), "b".into()]);
        q.clear();
        assert!(q.tracks().is_empty());
    }

    #[test]
    fn test_skip_to() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into(), "d".into()]);
        let track = q.skip_to(2);
        assert_eq!(track, Some("c".into()));
        assert_eq!(q.tracks(), vec!["d"]);
    }

    #[test]
    fn test_skip_to_out_of_bounds() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into()]);
        assert_eq!(q.skip_to(5), None);
        assert_eq!(q.tracks(), vec!["a"]);
    }

    #[test]
    fn test_set_current_moves_to_previous() {
        let mut q = PlaybackQueue::new();
        q.set_current("track1".into());
        assert_eq!(q.current_track_id(), Some("track1"));
        assert_eq!(q.previous_track_id(), None);

        q.set_current("track2".into());
        assert_eq!(q.current_track_id(), Some("track2"));
        assert_eq!(q.previous_track_id(), Some("track1"));
    }

    #[test]
    fn test_next_track_from_queue() {
        let mut q = PlaybackQueue::new();
        q.set_current("current".into());
        q.add_to_queue(vec!["next1".into(), "next2".into()]);
        match q.next_track() {
            NextTrack::Play(id) => assert_eq!(id, "next1"),
            _ => panic!("Expected Play"),
        }
        assert_eq!(q.previous_track_id(), Some("current"));
        assert_eq!(q.tracks(), vec!["next2"]);
    }

    #[test]
    fn test_next_track_repeat_current() {
        let mut q = PlaybackQueue::new();
        q.set_current("track1".into());
        q.set_repeat_mode(RepeatMode::Track);
        match q.next_track() {
            NextTrack::RepeatCurrent(id) => assert_eq!(id, "track1"),
            _ => panic!("Expected RepeatCurrent"),
        }
    }

    #[test]
    fn test_next_track_repeat_album_needed() {
        let mut q = PlaybackQueue::new();
        q.set_repeat_mode(RepeatMode::Album);
        match q.next_track() {
            NextTrack::RepeatAlbumNeeded => {}
            _ => panic!("Expected RepeatAlbumNeeded"),
        }
    }

    #[test]
    fn test_previous_action_restart_when_past_3s() {
        let q = PlaybackQueue::new();
        match q.previous_action(5000) {
            PreviousAction::RestartCurrent => {}
            _ => panic!("Expected RestartCurrent"),
        }
    }

    #[test]
    fn test_previous_action_go_back() {
        let mut q = PlaybackQueue::new();
        q.set_current("track1".into());
        q.set_current("track2".into());
        match q.previous_action(1000) {
            PreviousAction::PlayPrevious(id) => assert_eq!(id, "track1"),
            _ => panic!("Expected PlayPrevious"),
        }
    }

    #[test]
    fn test_previous_action_restart_when_no_previous() {
        let mut q = PlaybackQueue::new();
        q.set_current("track1".into());
        match q.previous_action(1000) {
            PreviousAction::RestartCurrent => {}
            _ => panic!("Expected RestartCurrent"),
        }
    }

    #[test]
    fn test_repeat_mode_default() {
        let q = PlaybackQueue::new();
        assert_eq!(q.repeat_mode(), RepeatMode::None);
    }

    #[test]
    fn test_insert_at_middle() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into()]);
        q.insert_at(1, vec!["x".into(), "y".into()]);
        assert_eq!(q.tracks(), vec!["a", "x", "y", "b", "c"]);
    }

    #[test]
    fn test_insert_at_beginning() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into(), "b".into()]);
        q.insert_at(0, vec!["x".into()]);
        assert_eq!(q.tracks(), vec!["x", "a", "b"]);
    }

    #[test]
    fn test_insert_at_end() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into(), "b".into()]);
        q.insert_at(2, vec!["x".into()]);
        assert_eq!(q.tracks(), vec!["a", "b", "x"]);
    }

    #[test]
    fn test_insert_at_beyond_end_clamps() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into()]);
        q.insert_at(999, vec!["x".into()]);
        assert_eq!(q.tracks(), vec!["a", "x"]);
    }

    #[test]
    fn test_replace() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["old".into()]);
        let mut new_queue = VecDeque::new();
        new_queue.push_back("new1".into());
        new_queue.push_back("new2".into());
        q.replace(new_queue);
        assert_eq!(q.tracks(), vec!["new1", "new2"]);
    }

    #[test]
    fn test_remove_by_ids_removes_matching_tracks() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into(), "d".into()]);
        let ids: HashSet<String> = ["b", "d"].iter().map(|s| s.to_string()).collect();
        q.remove_by_ids(&ids);
        assert_eq!(q.tracks(), vec!["a", "c"]);
    }

    #[test]
    fn test_remove_by_ids_clears_current_track() {
        let mut q = PlaybackQueue::new();
        q.set_current("track1".into());
        q.add_to_queue(vec!["a".into()]);
        let ids: HashSet<String> = ["track1"].iter().map(|s| s.to_string()).collect();
        q.remove_by_ids(&ids);
        assert_eq!(q.current_track_id(), None);
        assert_eq!(q.tracks(), vec!["a"]);
    }

    #[test]
    fn test_remove_by_ids_clears_previous_track() {
        let mut q = PlaybackQueue::new();
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
        let mut q = PlaybackQueue::new();
        q.set_current("current".into());
        q.add_to_queue(vec!["a".into(), "b".into()]);
        let ids: HashSet<String> = ["x", "y"].iter().map(|s| s.to_string()).collect();
        q.remove_by_ids(&ids);
        assert_eq!(q.tracks(), vec!["a", "b"]);
        assert_eq!(q.current_track_id(), Some("current"));
    }

    #[test]
    fn test_remove_by_ids_removes_all() {
        let mut q = PlaybackQueue::new();
        q.set_current("current".into());
        q.add_to_queue(vec!["a".into(), "b".into()]);
        let ids: HashSet<String> = ["a", "b", "current"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        q.remove_by_ids(&ids);
        assert!(q.tracks().is_empty());
        assert_eq!(q.current_track_id(), None);
    }

    #[test]
    fn test_next_track_repeat_track_without_current_falls_through() {
        // RepeatMode::Track only repeats when there IS a current track. With
        // none set, it must fall through to the normal advance.
        let mut q = PlaybackQueue::new();
        q.set_repeat_mode(RepeatMode::Track);
        q.add_to_queue(vec!["a".into(), "b".into()]);
        match q.next_track() {
            NextTrack::Play(id) => assert_eq!(id, "a"),
            _ => panic!("expected Play"),
        }
        // And with an empty queue (still no current) it stops rather than repeating.
        let mut q = PlaybackQueue::new();
        q.set_repeat_mode(RepeatMode::Track);
        assert!(matches!(q.next_track(), NextTrack::Stop));
    }

    #[test]
    fn test_reorder_same_index_is_noop() {
        let mut q = PlaybackQueue::new();
        q.add_to_queue(vec!["a".into(), "b".into(), "c".into()]);
        q.reorder(1, 1);
        assert_eq!(q.tracks(), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_front_peeks_without_removing() {
        let mut q = PlaybackQueue::new();
        assert_eq!(q.front(), None);
        q.add_to_queue(vec!["a".into(), "b".into()]);
        assert_eq!(q.front(), Some(&"a".to_string()));
        assert_eq!(q.len(), 2, "front must not consume the queue");
    }

    #[test]
    fn test_pop_front_empty_returns_none() {
        let mut q = PlaybackQueue::new();
        assert_eq!(q.pop_front(), None);
    }
}
