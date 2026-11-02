//! How much of what a load of the playing track waits on has come from
//! storage, for the surfaces that name that wait while the person waits on it.
//!
//! A load waits on the windows of its file the decoder reads first. Coven
//! tells a read that fetches from storage how many of the read's bytes are
//! here each time that grows; a read this device serves from its own copy or
//! the cache tells nothing, so a local load has no download to show.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::watch;

/// The bytes a load of the playing track waits on from storage: how many of
/// them are here, of how many, summed over the reads of its file in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackDownload {
    /// The track whose load waits.
    pub track_id: String,
    pub here: u64,
    pub total: u64,
}

/// The playing track and the buffer its first segment reads from: the reads
/// that buffer runs are the ones its load waits on.
struct Foreground {
    buffer_id: u64,
    track_id: String,
}

/// One read of a file from storage while it runs, once coven has told it.
struct Fetch {
    buffer_id: u64,
    here: u64,
    len: u64,
}

struct DownloadsState {
    foreground: Option<Foreground>,
    /// The track a load the person waits on is for; `None` while nothing
    /// loads.
    waiting_for: Option<String>,
    fetches: HashMap<u64, Fetch>,
    next_fetch: u64,
}

/// The playing track and the storage reads of every buffer, and from them
/// the download the playing track's load waits on, written to a watch each
/// time it changes.
pub(crate) struct TrackDownloads {
    state: Mutex<DownloadsState>,
    tx: watch::Sender<Option<TrackDownload>>,
}

impl TrackDownloads {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(DownloadsState {
                foreground: None,
                waiting_for: None,
                fetches: HashMap::new(),
                next_fetch: 0,
            }),
            tx: watch::channel(None).0,
        }
    }

    /// The download the playing track's load waits on: `None` while no load
    /// waits on storage.
    pub(crate) fn subscribe(&self) -> watch::Receiver<Option<TrackDownload>> {
        self.tx.subscribe()
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

    /// Where a read of `len` bytes from `buffer_id`'s file tells coven's
    /// progress, until the returned read is dropped.
    pub(crate) fn read(self: &Arc<Self>, buffer_id: u64, len: u64) -> FetchProgress {
        let id = {
            let mut state = self.lock();
            let id = state.next_fetch;
            state.next_fetch += 1;
            id
        };
        let downloads = self.clone();
        FetchProgress {
            downloads: self.clone(),
            id,
            progress: coven::ReadProgress::new(move |here| {
                downloads.told(id, buffer_id, len, here)
            }),
        }
    }

    /// Read `id`, of `len` bytes into `buffer_id`, has `here` of them.
    fn told(&self, id: u64, buffer_id: u64, len: u64, here: u64) {
        self.change(|state| {
            state.fetches.insert(id, Fetch { buffer_id, here, len });
        });
    }

    fn end_read(&self, id: u64) {
        self.change(|state| {
            state.fetches.remove(&id);
        });
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, DownloadsState> {
        self.state.lock().expect("track downloads lock poisoned")
    }

    /// Apply `edit`, then write the download it leaves when that differs
    /// from the one written.
    fn change(&self, edit: impl FnOnce(&mut DownloadsState)) {
        let mut state = self.lock();
        edit(&mut state);
        let download = waited_download(&state);
        self.tx.send_if_modified(|current| {
            if *current == download {
                return false;
            }
            *current = download;
            true
        });
    }
}

/// The download a load waits on: the playing track's storage reads while
/// the person waits on its load.
fn waited_download(state: &DownloadsState) -> Option<TrackDownload> {
    let foreground = state.foreground.as_ref()?;
    if state.waiting_for.as_deref() != Some(foreground.track_id.as_str()) {
        return None;
    }
    let (here, total) = state
        .fetches
        .values()
        .filter(|fetch| fetch.buffer_id == foreground.buffer_id)
        .fold(None, |sum: Option<(u64, u64)>, fetch| {
            let (here, total) = sum.unwrap_or((0, 0));
            Some((here + fetch.here, total + fetch.len))
        })?;
    Some(TrackDownload { track_id: foreground.track_id.clone(), here, total })
}

/// A read's progress, told to coven: counted toward the playing track's
/// download while the read runs, and no longer once this is dropped.
pub(crate) struct FetchProgress {
    downloads: Arc<TrackDownloads>,
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
        self.downloads.end_read(self.id);
    }
}

#[cfg(test)]
#[path = "track_download_tests.rs"]
mod tests;
