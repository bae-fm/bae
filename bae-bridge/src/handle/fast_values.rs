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
    /// fast its files go, into one stream by release id, and the files being
    /// sent together into another, both from one read of the outbox.
    pub fn feed_outbox_transfers(&self) -> crate::keyed_values::BridgeOutboxFeeds {
        crate::screens::outbox_transfer_feeds(self.runtime.clone(), self.run_feed())
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

    /// Write the share of each Activity transfer line's batch bytes done
    /// into a stream by line, under the key `BridgeActivityLine` gives it.
    /// A failed read empties the stream, and the app's next pull gets the
    /// error; the next bars fill the stream again.
    pub fn feed_activity_bars(&self) -> Arc<KeyedValueStream> {
        crate::screens::activity_bars(self.runtime.clone(), self.run_feed())
    }

    /// Write how far each pinning release has come into a stream by release
    /// id: its bytes and rate.
    pub fn feed_download_transfers(&self) -> Arc<KeyedValueStream> {
        crate::screens::download_transfers(self.runtime.clone(), self.run_feed())
    }

    /// Write the bytes of each release moving to this device and of its
    /// current file, by release id, and the share of each file, by file id.
    /// Both streams follow the same read of the active download.
    pub fn feed_move_to_this_device_transfers(
        &self,
    ) -> crate::keyed_values::BridgeMoveToThisDeviceFeeds {
        crate::screens::move_to_this_device_transfers(self.runtime.clone(), self.run_feed())
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
