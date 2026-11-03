//! Whether a load of the playing track waits on storage, for the players
//! that show that wait while the person waits on the load.
//!
//! A load waits on the windows of its file the decoder reads first. Coven
//! tells a read that fetches from storage how many of the read's bytes are
//! here each time that grows; a read this device serves from its own copy or
//! the cache tells nothing, so a local load waits on no storage.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::watch;

/// The playing track and the buffer its first segment reads from: the reads
/// that buffer runs are the ones its load waits on.
struct Foreground {
    buffer_id: u64,
    track_id: String,
}

struct WaitsState {
    foreground: Option<Foreground>,
    /// The track a load the person waits on is for; `None` while nothing
    /// loads.
    waiting_for: Option<String>,
    /// The buffer each running read fills, once coven has told the read it
    /// fetches from storage.
    fetching: HashMap<u64, u64>,
    next_read: u64,
}

/// The playing track and the storage reads of every buffer, and from them
/// the track whose load waits on storage, written to a watch each time it
/// changes.
pub(crate) struct StorageWaits {
    state: Mutex<WaitsState>,
    tx: watch::Sender<Option<String>>,
}

impl StorageWaits {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(WaitsState {
                foreground: None,
                waiting_for: None,
                fetching: HashMap::new(),
                next_read: 0,
            }),
            tx: watch::channel(None).0,
        }
    }

    /// Hear the playing track while its load waits on storage.
    pub(crate) fn subscribe(&self) -> StorageWaitWatch {
        StorageWaitWatch(self.tx.subscribe())
    }

    /// Make `track_id`, reading from `buffer_id`, the playing track.
    pub(crate) fn set_foreground(&self, buffer_id: u64, track_id: &str) {
        self.change(|state| {
            state.foreground = Some(Foreground { buffer_id, track_id: track_id.to_string() })
        });
    }

    pub(crate) fn foreground_buffer(&self) -> Option<u64> {
        self.lock().foreground.as_ref().map(|foreground| foreground.buffer_id)
    }

    /// The track a load the person waits on is for, or `None` once nothing
    /// loads.
    pub(crate) fn set_waiting(&self, track_id: Option<&str>) {
        self.change(|state| state.waiting_for = track_id.map(str::to_string));
    }

    /// Where a read into `buffer_id`'s file hears from coven that it fetches
    /// from storage, until the returned read is dropped.
    pub(crate) fn read(self: &Arc<Self>, buffer_id: u64) -> FetchProgress {
        let id = {
            let mut state = self.lock();
            let id = state.next_read;
            state.next_read += 1;
            id
        };
        let waits = self.clone();
        FetchProgress {
            waits: self.clone(),
            id,
            progress: coven::ReadProgress::new(move |_here| waits.fetches(id, buffer_id)),
        }
    }

    /// Read `id`, into `buffer_id`, fetches from storage.
    fn fetches(&self, id: u64, buffer_id: u64) {
        self.change(|state| {
            state.fetching.insert(id, buffer_id);
        });
    }

    fn end_read(&self, id: u64) {
        self.change(|state| {
            state.fetching.remove(&id);
        });
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, WaitsState> {
        self.state.lock().expect("storage waits lock poisoned")
    }

    /// Apply `edit`, then write the waiting track it leaves when that
    /// differs from the one written.
    fn change(&self, edit: impl FnOnce(&mut WaitsState)) {
        let mut state = self.lock();
        edit(&mut state);
        let waiting = waiting_on_storage(&state);
        self.tx.send_if_modified(|current| {
            if *current == waiting {
                return false;
            }
            *current = waiting;
            true
        });
    }
}

/// The playing track, while the person waits on its load and one of its
/// reads fetches from storage.
fn waiting_on_storage(state: &WaitsState) -> Option<String> {
    let foreground = state.foreground.as_ref()?;
    if state.waiting_for.as_deref() != Some(foreground.track_id.as_str()) {
        return None;
    }
    state
        .fetching
        .values()
        .any(|buffer_id| *buffer_id == foreground.buffer_id)
        .then(|| foreground.track_id.clone())
}

/// Hears which track's load waits on storage, from the storage waits
/// that made it.
pub struct StorageWaitWatch(watch::Receiver<Option<String>>);

impl StorageWaitWatch {
    /// The playing track while its load waits on storage, by id, `None`
    /// while no load does; the value seen from here on.
    pub fn waiting(&mut self) -> Option<String> {
        self.0.borrow_and_update().clone()
    }

    /// Wait until the waiting track differs from the one last seen; an
    /// error once playback is gone.
    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        self.0.changed().await
    }

    /// Whether the waiting track changed since it was last seen; an error
    /// once playback is gone.
    pub fn has_changed(&self) -> Result<bool, watch::error::RecvError> {
        self.0.has_changed()
    }
}

/// A read's progress, told to coven: the read counts as a fetch from storage
/// once coven tells it anything, and no longer once this is dropped.
pub(crate) struct FetchProgress {
    waits: Arc<StorageWaits>,
    id: u64,
    progress: coven::ReadProgress,
}

impl FetchProgress {
    pub(crate) fn progress(&self) -> &coven::ReadProgress {
        &self.progress
    }
}

impl Drop for FetchProgress {
    fn drop(&mut self) {
        self.waits.end_read(self.id);
    }
}

#[cfg(test)]
#[path = "storage_wait_tests.rs"]
mod tests;
