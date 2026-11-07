//! `AppHandle`'s `feed_*` calls: each hands back a stream of values core
//! sends many times a second, built in [`crate::screens`], and runs the feed
//! that writes it as the library's work, until the host cancels or drops the
//! stream or the library closes. These are the only calls that carry such
//! values to a host.

use std::sync::Arc;

use super::*;
use crate::keyed_values::KeyedValueStream;

impl AppHandle {
    /// Where a stream's feed runs: as the library's work, until the stream
    /// is cancelled or dropped or the library closes.
    fn run_feed(&self) -> impl crate::screens::RunFeed + '_ {
        |stop, writes| self.deliver_until(stop, writes)
    }
}

#[uniffi::export]
impl AppHandle {
    /// Write how far each release with files being sent has got, and how
    /// fast its files go, into a stream by release id. A failed read empties
    /// the stream and is pulled as an error; the next read fills it again.
    pub fn feed_release_upload_transfers(&self) -> Arc<KeyedValueStream> {
        crate::screens::release_upload_transfers(self.runtime.clone(), self.run_feed())
    }

    /// Write the files being sent together into a stream's one key, `""`,
    /// while any file is being sent; with none, the stream holds nothing. A
    /// failed read empties the stream and is pulled as an error; the next
    /// read fills it again.
    pub fn feed_upload_total_transfer(&self) -> Arc<KeyedValueStream> {
        crate::screens::upload_total_transfer(self.runtime.clone(), self.run_feed())
    }

    /// Write the share of each file being sent its object has moved into
    /// a stream by file id.
    pub fn feed_upload_file_shares(&self) -> Arc<KeyedValueStream> {
        crate::screens::upload_file_shares(self.runtime.clone(), self.run_feed())
    }

    /// Write the share of each file being downloaded to keep whole its
    /// object has moved into a stream by file id.
    pub fn feed_pin_file_shares(&self) -> Arc<KeyedValueStream> {
        crate::screens::pin_file_shares(self.runtime.clone(), self.run_feed())
    }

    /// Write how far each downloading release has come into a stream by
    /// release id: a pin's bytes and rate, or, for a release being made local,
    /// the bytes of the file coven is downloading into the person's folder
    /// and of the whole release.
    pub fn feed_download_transfers(&self) -> Arc<KeyedValueStream> {
        crate::screens::download_transfers(self.runtime.clone(), self.run_feed())
    }
}

#[uniffi::export]
impl AppHandle {
    /// Write where the playing track is into a stream keyed by track id, the
    /// playing track's alone, with the seeks applied so far.
    pub fn feed_playback_positions(&self) -> Arc<KeyedValueStream> {
        crate::screens::playback_positions(self.runtime.clone(), self.run_feed())
    }

    /// Write the parts of the playing track whose bytes this device holds
    /// into a stream keyed by track id, the playing track's alone.
    pub fn feed_track_buffered(&self) -> Arc<KeyedValueStream> {
        crate::screens::track_buffered(self.runtime.clone(), self.run_feed())
    }

    /// Write that the playing track's load waits on storage into a stream
    /// keyed by track id, the playing track's alone, while the person waits
    /// on the load and it reads from storage. A load read from this device's
    /// own copy or the cache writes nothing.
    pub fn feed_track_storage_waits(&self) -> Arc<KeyedValueStream> {
        crate::screens::track_storage_waits(self.runtime.clone(), self.run_feed())
    }
}

#[cfg(desktop)]
#[uniffi::export]
impl AppHandle {
    /// Write where the preview is in its clip into a stream's one key, `""`,
    /// while a preview is loaded.
    pub fn feed_preview_position(&self) -> Arc<KeyedValueStream> {
        crate::screens::preview_position(self.runtime.clone(), self.run_feed())
    }

    /// Write how far through its step each running import is into a stream
    /// by candidate key.
    pub fn feed_import_percents(&self) -> Arc<KeyedValueStream> {
        crate::screens::import_percents(self.runtime.clone(), self.run_feed())
    }
}

// Gated on the target, not on `desktop`: bae-library gates `mod export` itself on
// the target.
#[cfg(desktop)]
#[uniffi::export]
impl AppHandle {
    /// Write each exporting release's percent into a stream by release id.
    pub fn feed_export_percents(&self) -> Arc<KeyedValueStream> {
        crate::screens::export_percents(self.runtime.clone(), self.run_feed())
    }
}
