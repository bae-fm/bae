//! Whether one of the playing track's reads fetches from storage, for
//! playback, which shows a load of that track waiting on storage while it
//! does.
//!
//! Coven tells a read that fetches from storage how many of the read's bytes
//! are here each time that grows; a read this device serves from its own copy
//! or the cache tells nothing, so a local track's reads fetch nothing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::watch;

/// The playing track and the buffers of the files it reads: the reads those
/// buffers run are the playing track's.
struct Foreground {
    buffer_ids: Vec<u64>,
    track_id: String,
}

struct FetchesState {
    foreground: Option<Foreground>,
    /// The buffer each running read fills, once coven has told the read it
    /// fetches from storage.
    fetching: HashMap<u64, u64>,
    next_read: u64,
}

/// The playing track and the storage reads of every buffer, and from them
/// the playing track while one of its reads fetches from storage, written to
/// a watch each time it changes.
pub(crate) struct StorageFetches {
    state: Mutex<FetchesState>,
    tx: watch::Sender<Option<String>>,
}

impl StorageFetches {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(FetchesState {
                foreground: None,
                fetching: HashMap::new(),
                next_read: 0,
            }),
            tx: watch::channel(None).0,
        }
    }

    /// A watch of the playing track while one of its reads fetches from
    /// storage. The watch holds these fetches, so their sender lives as long
    /// as it does.
    pub(crate) fn subscribe(self: &Arc<Self>) -> StorageFetchWatch {
        StorageFetchWatch { rx: self.tx.subscribe(), _fetches: self.clone() }
    }

    /// Make `track_id`, reading from `buffer_ids`, the playing track.
    pub(crate) fn set_foreground(&self, buffer_ids: Vec<u64>, track_id: &str) {
        self.change(|state| {
            state.foreground = Some(Foreground { buffer_ids, track_id: track_id.to_string() })
        });
    }

    /// Whether `buffer_id` is one the playing track reads from.
    pub(crate) fn is_foreground(&self, buffer_id: u64) -> bool {
        self.lock()
            .foreground
            .as_ref()
            .is_some_and(|foreground| foreground.buffer_ids.contains(&buffer_id))
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
        let fetches = self.clone();
        FetchProgress {
            fetches: self.clone(),
            id,
            progress: coven::ReadProgress::new(move |_here| fetches.fetches(id, buffer_id)),
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

    fn lock(&self) -> std::sync::MutexGuard<'_, FetchesState> {
        self.state.lock().expect("storage fetches lock poisoned")
    }

    /// Apply `edit`, then write the fetching track it leaves when that
    /// differs from the one written.
    fn change(&self, edit: impl FnOnce(&mut FetchesState)) {
        let mut state = self.lock();
        edit(&mut state);
        let fetching = fetching_from_storage(&state);
        self.tx.send_if_modified(|current| {
            if *current == fetching {
                return false;
            }
            *current = fetching;
            true
        });
    }
}

/// The playing track, while one of its reads fetches from storage.
fn fetching_from_storage(state: &FetchesState) -> Option<String> {
    let foreground = state.foreground.as_ref()?;
    state
        .fetching
        .values()
        .any(|buffer_id| foreground.buffer_ids.contains(buffer_id))
        .then(|| foreground.track_id.clone())
}

/// A watch of whether the playing track fetches from storage. It holds the
/// storage fetches it watches, so its channel stays open while it exists.
pub struct StorageFetchWatch {
    rx: watch::Receiver<Option<String>>,
    _fetches: Arc<StorageFetches>,
}

impl StorageFetchWatch {
    /// The playing track while one of its reads fetches from storage, by
    /// id, `None` while none does. Reading it leaves the value unseen, so
    /// [`Self::changed`] still returns for it.
    pub fn fetching(&self) -> Option<String> {
        self.rx.borrow().clone()
    }

    /// Wait until the fetching track differs from the one last seen, and
    /// mark it seen.
    pub async fn changed(&mut self) {
        self.rx.changed().await.expect("the watch holds the storage fetches that send to it");
    }
}

/// A read's progress, told to coven: the read counts as a fetch from storage
/// once coven tells it anything, and no longer once this is dropped.
pub(crate) struct FetchProgress {
    fetches: Arc<StorageFetches>,
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
        self.fetches.end_read(self.id);
    }
}

#[cfg(test)]
#[path = "storage_fetch_tests.rs"]
mod tests;
