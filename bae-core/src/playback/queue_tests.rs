use super::*;
use coven::SequentialIdProvider;
use std::sync::Arc;

fn queue() -> PlaybackQueue {
    PlaybackQueue::new(Arc::new(SequentialIdProvider::new("entry")))
}

/// The upcoming projection's track ids in order — the per-instance ids vary,
/// so most assertions are about which tracks sit where.
fn upcoming_tracks(q: &PlaybackQueue) -> Vec<String> {
    q.upcoming().into_iter().map(|e| e.track_id).collect()
}

/// The full play order from the current track onward: the current track then
/// the upcoming projection.
fn full_order(q: &PlaybackQueue) -> Vec<String> {
    let mut order = vec![q.current_track_id().unwrap().to_string()];
    order.extend(upcoming_tracks(q));
    order
}

fn manual_ids(q: &PlaybackQueue) -> Vec<QueueEntryId> {
    q.manual.iter().map(|e| e.id.clone()).collect()
}

fn rel(tracks: &[&str]) -> Vec<String> {
    tracks.iter().map(|s| s.to_string()).collect()
}

/// A release source for the given id, the common `play_release` source.
fn rel_src(id: &str) -> ContextSource {
    ContextSource::Release(id.to_string())
}

// -- manual lane -----------------------------------------------------------

include!("queue_tests/mutation.rs");
include!("queue_tests/persistence.rs");
include!("queue_tests/projection.rs");
