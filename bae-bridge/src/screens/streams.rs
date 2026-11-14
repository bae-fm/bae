//! The streams of values core sends many times a second, each written by
//! its feed until the app cancels or drops it or the library closes: the
//! only way such values reach an app. Each function here builds a stream and
//! the feed that writes it; the handle runs the feed as the library's work.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use bae_app::AppServices;
use bae_library::CancellationToken;

use crate::keyed_values::{BridgeOutboxFeeds, KeyedValueStream, StreamValues, StreamWriter};
use crate::operation_runtime::OperationRuntime;
use crate::types::{BridgeError, BridgeFastValue};

/// What a feed does to write its stream: from the library's services, lent
/// for the call, the watch it follows, and the writing that follows it.
pub(crate) type Writes =
    Box<dyn FnOnce(&AppServices) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

/// Where a feed's writes run: as the library's work, until the token it is
/// handed is cancelled, as cancelling or dropping the stream does.
pub(crate) trait RunFeed: FnOnce(CancellationToken, Writes) {}

impl<F: FnOnce(CancellationToken, Writes)> RunFeed for F {}

/// A new stream, its `writes` handed to `run`.
fn feed<Fut>(
    runtime: OperationRuntime,
    run: impl RunFeed,
    writes: impl FnOnce(&AppServices, StreamWriter) -> Fut + Send + 'static,
) -> Arc<KeyedValueStream>
where
    Fut: Future<Output = ()> + Send + 'static,
{
    let values = StreamValues::new();
    let stop = CancellationToken::new();
    let writer = StreamWriter::new(values.clone());
    run(stop.clone(), Box::new(move |services| Box::pin(writes(services, writer))));
    Arc::new(KeyedValueStream::new(values, stop, runtime))
}

/// Wait for `next`, the feed's read of what changed. Meanwhile every key of
/// `writers` whose value has gone unsaid for [`QUIET`](crate::keyed_values::QUIET) rests (a paused
/// download, a held bar: the watches say a value only when it changes), so a
/// view stops reading it each frame until it moves again.
async fn resting_while<T>(writers: &[&StreamWriter], next: impl Future<Output = T>) -> T {
    let mut next = std::pin::pin!(next);
    let rest = || writers.iter().filter_map(|writer| writer.rest_quiet()).min();
    let mut next_rest = rest();
    loop {
        let Some(at) = next_rest else { return next.await };
        tokio::select! {
            item = &mut next => return item,
            () = tokio::time::sleep_until(at) => next_rest = rest(),
        }
    }
}

/// Write each batch of a release transfer watch: every release a `Reset`
/// names, then each one that moved or left.
fn write_release_changes<Progress>(
    writer: &StreamWriter,
    changes: Vec<bae_transfers::ReleaseTransferChange<Progress>>,
    value: impl Fn(Progress) -> BridgeFastValue,
) {
    use bae_transfers::ReleaseTransferChange;
    for change in changes {
        match change {
            ReleaseTransferChange::Updated { release_id, progress } => {
                writer.set(release_id, value(progress))
            }
            ReleaseTransferChange::Removed { release_id } => writer.remove(release_id.as_bytes()),
            ReleaseTransferChange::Reset(all) => writer.reset(
                all.into_iter()
                    .map(|(release_id, progress)| (release_id.into_bytes(), value(progress)))
                    .collect(),
            ),
        }
    }
}

/// The files being sent together as their stream holds them: only while
/// any is being sent.
fn upload_total(transfer: bae_library::UploadTransfer) -> Option<BridgeFastValue> {
    transfer.bar.is_some().then(|| BridgeFastValue::UploadTotal {
        transfer: crate::types::BridgeUploadTransfer::from_core(transfer),
    })
}

/// Write each batch of a file transfer watch, keyed by file id.
fn write_file_changes(writer: &StreamWriter, changes: Vec<bae_library::FileTransferChange>) {
    use bae_library::FileTransferChange;
    let share = |fraction| BridgeFastValue::FileShare { fraction };
    for change in changes {
        match change {
            FileTransferChange::Updated { file, fraction } => {
                writer.set(file.to_string(), share(fraction))
            }
            FileTransferChange::Removed { file } => writer.remove(file.to_string().as_bytes()),
            FileTransferChange::Reset(files) => writer.reset(
                files
                    .into_iter()
                    .map(|(file, fraction)| (file.to_string().into_bytes(), share(fraction)))
                    .collect(),
            ),
        }
    }
}

/// Write each batch of the outbox's transfers: the releases' into their
/// stream and the files being sent together into theirs.
fn write_outbox_changes(
    releases: &StreamWriter,
    total: &StreamWriter,
    changes: Vec<bae_library::OutboxTransferChange>,
) {
    use bae_library::OutboxTransferChange;
    let upload = |transfer| BridgeFastValue::ReleaseUpload {
        transfer: crate::types::BridgeReleaseUploadTransfer::from_core(transfer),
    };
    let mut release_changes = Vec::new();
    for change in changes {
        match change {
            OutboxTransferChange::Release(change) => release_changes.push(change),
            OutboxTransferChange::Total(transfer) => match upload_total(transfer) {
                Some(value) => total.set_only(value),
                None => total.remove_only(),
            },
        }
    }
    write_release_changes(releases, release_changes, upload);
}

/// Two new streams written by one feed, which ends once the host has
/// cancelled or dropped both, or the library closes. Each stream is cancelled
/// on its own.
fn feed_pair<Fut>(
    runtime: OperationRuntime,
    run: impl RunFeed,
    writes: impl FnOnce(&AppServices, StreamWriter, StreamWriter) -> Fut + Send + 'static,
) -> (Arc<KeyedValueStream>, Arc<KeyedValueStream>)
where
    Fut: Future<Output = ()> + Send + 'static,
{
    let (first_values, second_values) = (StreamValues::new(), StreamValues::new());
    let (first_stop, second_stop) = (CancellationToken::new(), CancellationToken::new());
    let writers =
        (StreamWriter::new(first_values.clone()), StreamWriter::new(second_values.clone()));
    let both_cancelled = (first_stop.clone(), second_stop.clone());
    run(
        CancellationToken::new(),
        Box::new(move |services| {
            let writing = writes(services, writers.0, writers.1);
            Box::pin(async move {
                tokio::select! {
                    () = async {
                        both_cancelled.0.cancelled().await;
                        both_cancelled.1.cancelled().await;
                    } => {}
                    () = writing => {}
                }
            })
        }),
    );
    (
        Arc::new(KeyedValueStream::new(first_values, first_stop, runtime.clone())),
        Arc::new(KeyedValueStream::new(second_values, second_stop, runtime)),
    )
}

/// The streams [`AppHandle::feed_outbox_transfers`](crate::AppHandle::feed_outbox_transfers) hands out,
/// written by one read of the outbox's transfers.
pub(crate) fn outbox_transfer_feeds(
    runtime: OperationRuntime,
    run: impl RunFeed,
) -> BridgeOutboxFeeds {
    let (releases, total) = feed_pair(runtime, run, move |services, releases, total| {
        let mut transfers = services.watch_outbox_transfers();
        async move {
            while let Some(changes) = resting_while(&[&releases, &total], transfers.next()).await {
                let changes = match changes {
                    Ok(changes) => changes,
                    Err(error) => {
                        let error = BridgeError::database_query(error);
                        releases.fail(error.clone());
                        total.fail(error);
                        continue;
                    }
                };
                write_outbox_changes(&releases, &total, changes);
            }
        }
    });
    BridgeOutboxFeeds::Streams { releases, total }
}

/// The stream [`AppHandle::feed_upload_file_shares`](crate::AppHandle::feed_upload_file_shares) hands out.
pub(crate) fn upload_file_shares(
    runtime: OperationRuntime,
    run: impl RunFeed,
) -> Arc<KeyedValueStream> {
    feed(runtime, run, move |services, writer| {
        let mut files = services.watch_upload_file_transfers();
        async move {
            while let Some(changes) = resting_while(&[&writer], files.next()).await {
                write_file_changes(&writer, changes);
            }
        }
    })
}

/// The stream [`AppHandle::feed_pin_file_shares`](crate::AppHandle::feed_pin_file_shares) hands out.
pub(crate) fn pin_file_shares(
    runtime: OperationRuntime,
    run: impl RunFeed,
) -> Arc<KeyedValueStream> {
    feed(runtime, run, move |services, writer| {
        let mut files = services.watch_pin_file_transfers();
        async move {
            while let Some(changes) = resting_while(&[&writer], files.next()).await {
                write_file_changes(&writer, changes);
            }
        }
    })
}

/// The stream [`AppHandle::feed_activity_bars`](crate::AppHandle::feed_activity_bars) hands out.
pub(crate) fn activity_bars(runtime: OperationRuntime, run: impl RunFeed) -> Arc<KeyedValueStream> {
    feed(runtime, run, move |services, writer| {
        let mut bars = services.watch_activity_bars();
        async move {
            while let Some(read) = resting_while(&[&writer], bars.next()).await {
                match read {
                    Ok(bars) => writer.reset(activity_bars_held(bars)),
                    Err(error) => writer.fail(BridgeError::database_query(error)),
                }
            }
        }
    })
}

/// Each transfer line's bar as their stream holds it: the share of its
/// batch's bytes done under the line's name, while its batch holds a
/// release.
fn activity_bars_held(bars: bae_library::ActivityBars) -> HashMap<Vec<u8>, BridgeFastValue> {
    let bae_library::ActivityBars { uploads, pins, moving_to_this_device } = bars;
    [("uploads", uploads), ("pins", pins), ("moving_to_this_device", moving_to_this_device)]
        .into_iter()
        .filter_map(|(line, bar)| {
            bar.map(|bar| {
                (line.as_bytes().to_vec(), BridgeFastValue::BatchShare { fraction: bar.fraction() })
            })
        })
        .collect()
}

/// What a download watch's changes say to a stream of one kind of download:
/// the releases doing that kind, and a release that stops doing it, whether
/// it ended or turned to the other kind, as removed.
fn download_changes<Progress>(
    changes: Vec<bae_transfers::ReleaseTransferChange<bae_transfers::DownloadTransfer>>,
    kind: impl Fn(bae_transfers::DownloadTransfer) -> Option<Progress>,
) -> Vec<bae_transfers::ReleaseTransferChange<Progress>> {
    use bae_transfers::ReleaseTransferChange;
    changes
        .into_iter()
        .map(|change| match change {
            ReleaseTransferChange::Updated { release_id, progress } => match kind(progress) {
                Some(progress) => ReleaseTransferChange::Updated { release_id, progress },
                None => ReleaseTransferChange::Removed { release_id },
            },
            ReleaseTransferChange::Removed { release_id } => {
                ReleaseTransferChange::Removed { release_id }
            }
            ReleaseTransferChange::Reset(all) => ReleaseTransferChange::Reset(
                all.into_iter()
                    .filter_map(|(release_id, progress)| {
                        kind(progress).map(|progress| (release_id, progress))
                    })
                    .collect(),
            ),
        })
        .collect()
}

/// The pin of a download, if that is what it is.
fn pin_progress(
    transfer: bae_transfers::DownloadTransfer,
) -> Option<bae_transfers::DownloadTransferProgress> {
    match transfer {
        bae_transfers::DownloadTransfer::Pin(progress) => Some(progress),
        bae_transfers::DownloadTransfer::MoveToThisDevice(_) => None,
    }
}

/// The move-to-this-device of a download, if that is what it is.
fn move_to_this_device_progress(
    transfer: bae_transfers::DownloadTransfer,
) -> Option<bae_transfers::MoveToThisDeviceTransferProgress> {
    match transfer {
        bae_transfers::DownloadTransfer::MoveToThisDevice(progress) => Some(progress),
        bae_transfers::DownloadTransfer::Pin(_) => None,
    }
}

/// The stream [`AppHandle::feed_download_transfers`](crate::AppHandle::feed_download_transfers) hands out.
pub(crate) fn download_transfers(
    runtime: OperationRuntime,
    run: impl RunFeed,
) -> Arc<KeyedValueStream> {
    feed(runtime, run, move |services, writer| {
        let mut transfers = services.watch_download_transfers();
        async move {
            while let Some(changes) = resting_while(&[&writer], transfers.next()).await {
                write_release_changes(
                    &writer,
                    download_changes(changes, pin_progress),
                    |progress| BridgeFastValue::Download {
                        progress: crate::types::BridgeDownloadTransferProgress::from_core(progress),
                    },
                );
            }
        }
    })
}

/// The stream [`AppHandle::feed_move_to_this_device_transfers`](crate::AppHandle::feed_move_to_this_device_transfers) hands out.
pub(crate) fn move_to_this_device_transfers(
    runtime: OperationRuntime,
    run: impl RunFeed,
) -> Arc<KeyedValueStream> {
    feed(runtime, run, move |services, writer| {
        let mut transfers = services.watch_download_transfers();
        async move {
            while let Some(changes) = resting_while(&[&writer], transfers.next()).await {
                write_release_changes(
                    &writer,
                    download_changes(changes, move_to_this_device_progress),
                    |progress| BridgeFastValue::MoveToThisDevice {
                        progress: crate::types::BridgeMoveToThisDeviceProgress::from_core(progress),
                    },
                );
            }
        }
    })
}

/// The stream [`AppHandle::feed_playback_positions`](crate::AppHandle::feed_playback_positions) hands out.
pub(crate) fn playback_positions(
    runtime: OperationRuntime,
    run: impl RunFeed,
) -> Arc<KeyedValueStream> {
    feed(runtime, run, move |services, writer| {
        let mut positions = services.subscribe_playback_positions();
        async move {
            let mut fold = bae_playback::AnchorFold::new();
            loop {
                write_track_anchor(
                    &mut fold,
                    &writer,
                    &positions.positions(),
                    bae_util::frame_clock::frame_clock_ns(),
                );
                if positions.changed().await.is_err() {
                    return;
                }
            }
        }
    })
}

/// The stream [`AppHandle::feed_track_buffered`](crate::AppHandle::feed_track_buffered) hands out.
pub(crate) fn track_buffered(
    runtime: OperationRuntime,
    run: impl RunFeed,
) -> Arc<KeyedValueStream> {
    feed(runtime, run, move |services, writer| {
        let mut buffered = services.follow_track_buffered();
        async move {
            while let Some(held) = buffered.next().await {
                writer.reset_resting(track_buffered_held(held), |_, value| {
                    track_is_fully_buffered(value)
                });
            }
        }
    })
}

/// The stream [`AppHandle::feed_track_storage_waits`](crate::AppHandle::feed_track_storage_waits) hands out.
pub(crate) fn track_storage_waits(
    runtime: OperationRuntime,
    run: impl RunFeed,
) -> Arc<KeyedValueStream> {
    feed(runtime, run, move |services, writer| {
        let mut waits = services.subscribe_storage_waits();
        async move {
            loop {
                write_storage_wait(&writer, waits.waiting());
                if waits.changed().await.is_err() {
                    return;
                }
            }
        }
    })
}

#[cfg(desktop)]
/// The stream [`AppHandle::feed_preview_position`](crate::AppHandle::feed_preview_position) hands out.
pub(crate) fn preview_position(
    runtime: OperationRuntime,
    run: impl RunFeed,
) -> Arc<KeyedValueStream> {
    feed(runtime, run, move |services, writer| {
        let mut positions = services.subscribe_playback_positions();
        async move {
            loop {
                let held = positions.positions();
                match held.preview {
                    Some(position) => writer.set_only(BridgeFastValue::PreviewPosition {
                        position: crate::types::BridgePreviewPosition::from_core(position),
                    }),
                    None => writer.remove_only(),
                }
                if positions.changed().await.is_err() {
                    return;
                }
            }
        }
    })
}

#[cfg(desktop)]
/// The stream [`AppHandle::feed_import_percents`](crate::AppHandle::feed_import_percents) hands out.
pub(crate) fn import_percents(
    runtime: OperationRuntime,
    run: impl RunFeed,
) -> Arc<KeyedValueStream> {
    feed(runtime, run, move |services, writer| {
        let mut percents = services.watch_import_percents();
        async move {
            loop {
                let held: HashMap<Vec<u8>, BridgeFastValue> = percents
                    .percents()
                    .iter()
                    .map(|(key, percent)| {
                        (
                            key.as_bytes().to_vec(),
                            BridgeFastValue::ImportPercent { percent: *percent },
                        )
                    })
                    .collect();
                writer.reset(held);
                if percents.changed().await.is_err() {
                    return;
                }
            }
        }
    })
}

#[cfg(desktop)]
/// The stream [`AppHandle::feed_tab_counts`](crate::AppHandle::feed_tab_counts) hands out.
pub(crate) fn tab_counts(runtime: OperationRuntime, run: impl RunFeed) -> Arc<KeyedValueStream> {
    feed(runtime, run, move |services, writer| {
        let counts = services.subscribe_tab_counts();
        async move {
            loop {
                match counts.next().await {
                    Ok(counts) => writer.reset(tab_counts_held(counts)),
                    Err(bae_store::live_query::LiveQueryError::Cancelled) => return,
                    Err(error) => writer.fail(BridgeError::from(error)),
                }
            }
        }
    })
}

#[cfg(desktop)]
/// The stream [`AppHandle::feed_scan_found_counts`](crate::AppHandle::feed_scan_found_counts) hands out.
pub(crate) fn scan_found_counts(
    runtime: OperationRuntime,
    run: impl RunFeed,
) -> Arc<KeyedValueStream> {
    feed(runtime, run, move |services, writer| {
        let counts = services.subscribe_scan_found_counts();
        async move {
            loop {
                match counts.next().await {
                    Ok(counts) => writer.reset(scan_found_held(counts)),
                    Err(bae_store::live_query::LiveQueryError::Cancelled) => return,
                    Err(error) => writer.fail(BridgeError::from(error)),
                }
            }
        }
    })
}

// Gated on the target, not on `desktop`: bae-library gates `mod export` itself on
// the target.
#[cfg(desktop)]
/// The stream [`AppHandle::feed_export_percents`](crate::AppHandle::feed_export_percents) hands out.
pub(crate) fn export_percents(
    runtime: OperationRuntime,
    run: impl RunFeed,
) -> Arc<KeyedValueStream> {
    feed(runtime, run, move |services, writer| {
        let mut transfers = services.watch_output_transfers();
        async move {
            while let Some(changes) = resting_while(&[&writer], transfers.next()).await {
                write_release_changes(&writer, changes, |percent| BridgeFastValue::ExportPercent {
                    percent,
                });
            }
        }
    })
}

/// Each tab's count as their stream holds them: under the tab's name, all
/// three always.
#[cfg(desktop)]
fn tab_counts_held(counts: bae_import::TriageTabCounts) -> HashMap<Vec<u8>, BridgeFastValue> {
    let bae_import::TriageTabCounts { pending, done, skipped } = counts;
    HashMap::from([
        (b"pending".to_vec(), BridgeFastValue::TabCount { count: pending }),
        (b"done".to_vec(), BridgeFastValue::TabCount { count: done }),
        (b"skipped".to_vec(), BridgeFastValue::TabCount { count: skipped }),
    ])
}

/// Each scan under way's stored folders as their stream holds them: under
/// its root's path.
#[cfg(desktop)]
fn scan_found_held(counts: bae_import::ScanFoundCounts) -> HashMap<Vec<u8>, BridgeFastValue> {
    counts
        .into_iter()
        .map(|(root, count)| (coven::path_bytes(&root), BridgeFastValue::ScanFoundCount { count }))
        .collect()
}

/// Write the playing track's anchor, read at `at_ns` on the frame clock, when
/// `positions` leave the line the stream holds; the position ticks along it
/// write nothing. An anchor at rate zero (a paused track) is at rest.
fn write_track_anchor(
    fold: &mut bae_playback::AnchorFold,
    writer: &StreamWriter,
    positions: &bae_playback::PlaybackPositions,
    at_ns: u64,
) {
    if fold.fold(positions, at_ns) {
        writer.reset_resting(track_anchor_held(fold.anchor()), |_, value| {
            matches!(value, BridgeFastValue::TrackPosition { position } if position.rate == 0.0)
        });
    }
}

/// The playing track's anchor as its stream holds it: under its track's key,
/// or nothing.
fn track_anchor_held(
    anchor: Option<&bae_playback::TrackAnchor>,
) -> HashMap<Vec<u8>, BridgeFastValue> {
    anchor
        .map(|anchor| {
            (
                anchor.track_id.clone().into_bytes(),
                BridgeFastValue::TrackPosition {
                    position: crate::types::BridgeTrackPosition::from_core(anchor.clone()),
                },
            )
        })
        .into_iter()
        .collect()
}

/// The playing track's held stretches as their stream holds them: under its
/// track's key, or nothing while no track plays.
fn track_buffered_held(
    buffered: Option<bae_playback::TrackBuffered>,
) -> HashMap<Vec<u8>, BridgeFastValue> {
    buffered
        .map(|buffered| {
            (
                buffered.track_id.into_bytes(),
                BridgeFastValue::TrackBuffered {
                    buffered: crate::types::BridgeTrackBuffered {
                        ranges: buffered
                            .ranges
                            .into_iter()
                            .map(crate::types::BridgeTrackSpan::from_core)
                            .collect(),
                    },
                },
            )
        })
        .into_iter()
        .collect()
}

/// Whether `value` is a track held from its start to its end in one stretch,
/// which no longer changes.
fn track_is_fully_buffered(value: &BridgeFastValue) -> bool {
    matches!(
        value,
        BridgeFastValue::TrackBuffered { buffered }
            if matches!(buffered.ranges.as_slice(), [span] if span.start == 0.0 && span.end == 1.0)
    )
}

/// The stream holds the track whose load waits on storage, at rest: the wait
/// is the whole value, so a view reads it once when it comes, and the arc it
/// shows turns on its own clock.
fn write_storage_wait(writer: &StreamWriter, waiting: Option<String>) {
    writer.reset_resting(track_storage_wait(waiting), |_, _| true);
}

/// The track whose load waits on storage as its stream holds it: under its
/// key, or nothing while no load does.
fn track_storage_wait(waiting: Option<String>) -> HashMap<Vec<u8>, BridgeFastValue> {
    waiting
        .map(|track_id| (track_id.into_bytes(), BridgeFastValue::TrackWaitsOnStorage))
        .into_iter()
        .collect()
}

#[cfg(test)]
#[path = "streams_tests.rs"]
mod tests;
