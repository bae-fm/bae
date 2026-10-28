//! `AppHandle`'s `feed_*` calls: each starts writing one stream of values
//! core sends many times a second and hands back the `KeyedValueStream` it
//! writes, until the host cancels or drops it or the library closes. These
//! are the only calls that carry such values to a host.

use std::collections::HashMap;
use std::sync::Arc;

use super::*;
use crate::keyed_values::{KeyedValueStream, StreamValues, StreamWriter};
use crate::types::{BridgeFastValue, KeyActivityCallback, StreamFailureCallback};

/// The key of a stream that holds one value.
const ONLY_KEY: &str = "";

impl AppHandle {
    /// Run `feed` writing a new stream as the library's work, and hand back
    /// the stream, which stops the feed when it is cancelled or dropped.
    fn feed<Fut>(
        &self,
        activity: Box<dyn KeyActivityCallback>,
        feed: impl FnOnce(AppServices, StreamWriter) -> Fut + Send + 'static,
    ) -> Arc<KeyedValueStream>
    where
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let values = StreamValues::new(activity);
        let stop = bae_core::library::CancellationToken::new();
        let writer = StreamWriter::new(values.clone());
        self.deliver_until(stop.clone(), move |services| feed(services, writer));
        Arc::new(KeyedValueStream::new(values, stop))
    }
}

/// Write each batch of a release transfer watch: every release a `Reset`
/// names, then each one that moved or left.
fn write_release_changes<Progress>(
    writer: &StreamWriter,
    changes: Vec<bae_core::library::ReleaseTransferChange<Progress>>,
    value: impl Fn(Progress) -> BridgeFastValue,
) {
    use bae_core::library::ReleaseTransferChange;
    for change in changes {
        match change {
            ReleaseTransferChange::Updated {
                release_id,
                progress,
            } => writer.set(release_id, value(progress)),
            ReleaseTransferChange::Removed { release_id } => writer.remove(&release_id),
            ReleaseTransferChange::Reset(all) => writer.reset(
                all.into_iter()
                    .map(|(release_id, progress)| (release_id, value(progress)))
                    .collect(),
            ),
        }
    }
}

/// The files being sent together as their stream holds them: only while
/// any is being sent.
fn upload_total(transfer: bae_core::library::UploadTransfer) -> Option<BridgeFastValue> {
    transfer
        .bar
        .is_some()
        .then(|| BridgeFastValue::UploadTotal {
            transfer: crate::types::BridgeUploadTransfer::from_core(transfer),
        })
}

/// Write each batch of a file transfer watch, keyed by file id.
fn write_file_changes(writer: &StreamWriter, changes: Vec<bae_core::library::FileTransferChange>) {
    use bae_core::library::FileTransferChange;
    let share = |fraction| BridgeFastValue::FileShare { fraction };
    for change in changes {
        match change {
            FileTransferChange::Updated { file, fraction } => {
                writer.set(file.to_string(), share(fraction))
            }
            FileTransferChange::Removed { file } => writer.remove(&file.to_string()),
            FileTransferChange::Reset(files) => writer.reset(
                files
                    .into_iter()
                    .map(|(file, fraction)| (file.to_string(), share(fraction)))
                    .collect(),
            ),
        }
    }
}

#[uniffi::export]
impl AppHandle {
    /// Write how far each release with files being sent has got, and how
    /// fast its files go, into a stream by release id. A failed read empties
    /// the stream and tells `failure`; the next read fills it again.
    pub fn feed_release_upload_transfers(
        &self,
        activity: Box<dyn KeyActivityCallback>,
        failure: Box<dyn StreamFailureCallback>,
    ) -> Arc<KeyedValueStream> {
        self.feed(activity, move |services, writer| async move {
            use bae_core::library::OutboxTransferChange;
            let upload = |transfer| BridgeFastValue::ReleaseUpload {
                transfer: crate::types::BridgeReleaseUploadTransfer::from_core(transfer),
            };
            let mut transfers = services.watch_outbox_transfers();
            while let Some(changes) = transfers.next().await {
                let changes = match changes {
                    Ok(changes) => changes,
                    Err(error) => {
                        writer.reset(HashMap::new());
                        failure.on_error(BridgeError::database_query(error));
                        continue;
                    }
                };
                for change in changes {
                    match change {
                        OutboxTransferChange::Updated {
                            release_id,
                            transfer,
                        } => writer.set(release_id, upload(transfer)),
                        OutboxTransferChange::Removed { release_id } => writer.remove(&release_id),
                        // The total is its own stream.
                        OutboxTransferChange::TotalUpdated(_) => {}
                        OutboxTransferChange::Reset(all) => writer.reset(
                            all.releases
                                .into_iter()
                                .map(|(release_id, transfer)| (release_id, upload(transfer)))
                                .collect(),
                        ),
                    }
                }
            }
        })
    }

    /// Write the files being sent together into a stream's one key, `""`,
    /// while any file is being sent; with none, the stream holds nothing. A
    /// failed read empties the stream and tells `failure`; the next read
    /// fills it again.
    pub fn feed_upload_total_transfer(
        &self,
        activity: Box<dyn KeyActivityCallback>,
        failure: Box<dyn StreamFailureCallback>,
    ) -> Arc<KeyedValueStream> {
        self.feed(activity, move |services, writer| async move {
            let mut total = services.watch_outbox_total_transfer();
            while let Some(transfer) = total.next().await {
                match transfer.map(upload_total) {
                    Ok(Some(total)) => writer.set(ONLY_KEY.to_string(), total),
                    Ok(None) => writer.remove(ONLY_KEY),
                    Err(error) => {
                        writer.remove(ONLY_KEY);
                        failure.on_error(BridgeError::database_query(error));
                    }
                }
            }
        })
    }

    /// Write the share of each file being sent its object has moved into
    /// a stream by file id.
    pub fn feed_upload_file_shares(
        &self,
        activity: Box<dyn KeyActivityCallback>,
    ) -> Arc<KeyedValueStream> {
        self.feed(activity, move |services, writer| async move {
            let mut files = services.watch_upload_file_transfers();
            while let Some(changes) = files.next().await {
                write_file_changes(&writer, changes);
            }
        })
    }

    /// Write the share of each file being downloaded to keep whole its
    /// object has moved into a stream by file id.
    pub fn feed_pin_file_shares(
        &self,
        activity: Box<dyn KeyActivityCallback>,
    ) -> Arc<KeyedValueStream> {
        self.feed(activity, move |services, writer| async move {
            let mut files = services.watch_pin_file_transfers();
            while let Some(changes) = files.next().await {
                write_file_changes(&writer, changes);
            }
        })
    }

    /// Write each downloading release's bytes and rate into a stream by
    /// release id.
    pub fn feed_download_transfers(
        &self,
        activity: Box<dyn KeyActivityCallback>,
    ) -> Arc<KeyedValueStream> {
        self.feed(activity, move |services, writer| async move {
            let mut transfers = services.watch_download_transfers();
            while let Some(changes) = transfers.next().await {
                write_release_changes(&writer, changes, |progress| BridgeFastValue::Download {
                    progress: crate::types::BridgeDownloadTransferProgress::from_core(progress),
                });
            }
        })
    }
}

#[uniffi::export]
impl AppHandle {
    /// Write where the playing track is into a stream keyed by track id, the
    /// playing track's alone, with the seeks applied so far.
    pub fn feed_playback_positions(
        &self,
        activity: Box<dyn KeyActivityCallback>,
    ) -> Arc<KeyedValueStream> {
        self.feed(activity, move |services, writer| async move {
            let mut positions = services.subscribe_playback_positions();
            loop {
                let held = track_positions(&positions.borrow_and_update());
                writer.reset(held);
                if positions.changed().await.is_err() {
                    return;
                }
            }
        })
    }
}

/// The playing track's position as its stream holds it: under its track's
/// key, or nothing.
fn track_positions(
    positions: &bae_core::playback::PlaybackPositions,
) -> HashMap<String, BridgeFastValue> {
    positions
        .library
        .clone()
        .map(|position| {
            (
                position.track_id.clone(),
                BridgeFastValue::TrackPosition {
                    position: crate::types::BridgeTrackPosition::from_core(
                        position,
                        positions.seek_revision,
                    ),
                },
            )
        })
        .into_iter()
        .collect()
}

#[cfg(feature = "desktop")]
#[uniffi::export]
impl AppHandle {
    /// Write where the preview is in its clip into a stream's one key, `""`,
    /// while a preview is loaded.
    pub fn feed_preview_position(
        &self,
        activity: Box<dyn KeyActivityCallback>,
    ) -> Arc<KeyedValueStream> {
        self.feed(activity, move |services, writer| async move {
            let mut positions = services.subscribe_playback_positions();
            loop {
                let preview = positions.borrow_and_update().preview.clone();
                match preview {
                    Some(position) => writer.set(
                        ONLY_KEY.to_string(),
                        BridgeFastValue::PreviewPosition {
                            position: crate::types::BridgePreviewPosition::from_core(position),
                        },
                    ),
                    None => writer.remove(ONLY_KEY),
                }
                if positions.changed().await.is_err() {
                    return;
                }
            }
        })
    }

    /// Write how far through its step each running import is into a stream
    /// by candidate key.
    pub fn feed_import_percents(
        &self,
        activity: Box<dyn KeyActivityCallback>,
    ) -> Arc<KeyedValueStream> {
        self.feed(activity, move |services, writer| async move {
            let mut percents = services.watch_import_percents();
            loop {
                let held: HashMap<String, BridgeFastValue> = percents
                    .borrow_and_update()
                    .iter()
                    .map(|(key, percent)| {
                        (
                            key.clone(),
                            BridgeFastValue::ImportPercent { percent: *percent },
                        )
                    })
                    .collect();
                writer.reset(held);
                if percents.changed().await.is_err() {
                    return;
                }
            }
        })
    }
}

// Gated on the target, not on `desktop`: bae-core gates `mod export` itself on
// the target.
#[cfg(desktop)]
#[uniffi::export]
impl AppHandle {
    /// Write each exporting release's percent into a stream by release id.
    pub fn feed_export_percents(
        &self,
        activity: Box<dyn KeyActivityCallback>,
    ) -> Arc<KeyedValueStream> {
        self.feed(activity, move |services, writer| async move {
            let mut transfers = services.watch_output_transfers();
            while let Some(changes) = transfers.next().await {
                write_release_changes(&writer, changes, |percent| BridgeFastValue::ExportPercent {
                    percent,
                });
            }
        })
    }
}

#[cfg(test)]
#[path = "fast_values_tests.rs"]
mod tests;
