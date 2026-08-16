use super::*;

/// Forward bus events to the platform callback until the bus closes.
///
/// Falling behind (`Lagged`) drops transient events but must not kill the
/// subscription. Persistent state is delivered by independent live-result
/// subscriptions and is unaffected by this bus.
pub(super) async fn pump_ui_events(
    mut rx: tokio::sync::broadcast::Receiver<bae_core::ui::UiBusEvent>,
    callback: Box<dyn crate::types::UiEventCallback>,
) {
    loop {
        match rx.recv().await {
            Ok(event) => {
                if let Some(bridge_event) = convert_ui_event(event) {
                    callback.on_event(bridge_event);
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("UI event subscription lagged; dropped {n} transient events");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}

impl crate::types::BridgeUploadReleaseGroup {
    pub(super) fn from_core(g: bae_core::library::UploadReleaseGroup) -> Self {
        let bae_core::library::UploadReleaseGroup {
            release_id,
            display_title,
            files,
            progress,
        } = g;
        Self {
            release_id,
            display_title,
            files: files
                .into_iter()
                .map(crate::types::BridgeUploadFileOp::from_core)
                .collect(),
            progress: crate::types::BridgeUploadProgress::from_core(progress),
        }
    }
}

impl crate::types::BridgeUploadFileOp {
    /// Flatten core's per-file `UploadState` into `state` + `bytes_done` +
    /// `last_error`, so the UI reads plain fields instead of switching on
    /// associated data.
    pub(super) fn from_core(f: bae_core::library::UploadFileOp) -> Self {
        use bae_core::library::UploadState;
        let bae_core::library::UploadFileOp {
            file_id,
            label,
            source_bytes_total,
            state,
        } = f;
        let (state, bytes_done, progress_bytes_total, last_error) = match state {
            UploadState::Queued => (crate::types::BridgeUploadFileState::Queued, 0, 0, None),
            UploadState::Preparing {
                bytes_done,
                bytes_total,
            } => (
                crate::types::BridgeUploadFileState::Preparing,
                bytes_done,
                bytes_total,
                None,
            ),
            UploadState::Prepared { bytes_total } => (
                crate::types::BridgeUploadFileState::Prepared,
                0,
                bytes_total,
                None,
            ),
            UploadState::Uploading {
                bytes_done,
                bytes_total,
            } => (
                crate::types::BridgeUploadFileState::Uploading,
                bytes_done,
                bytes_total,
                None,
            ),
            UploadState::RetryingPreparation { last_error } => (
                crate::types::BridgeUploadFileState::Retrying,
                0,
                source_bytes_total,
                Some(last_error),
            ),
            UploadState::RetryingUpload {
                last_error,
                bytes_total,
            } => (
                crate::types::BridgeUploadFileState::Retrying,
                0,
                bytes_total,
                Some(last_error),
            ),
            UploadState::RetryingPublication {
                last_error,
                bytes_total,
            } => (
                crate::types::BridgeUploadFileState::Retrying,
                bytes_total,
                bytes_total,
                Some(last_error),
            ),
            UploadState::Uploaded { bytes_total } => (
                crate::types::BridgeUploadFileState::Uploaded,
                bytes_total,
                bytes_total,
                None,
            ),
        };
        Self {
            file_id,
            label: crate::types::BridgeUploadFileLabel::from_core(label),
            bytes_done,
            progress_bytes_total,
            source_bytes_total,
            state,
            last_error,
        }
    }
}

impl crate::types::BridgeUploadFileLabel {
    fn from_core(label: bae_core::library::UploadFileLabel) -> Self {
        match label {
            bae_core::library::UploadFileLabel::Filename(name) => Self::Filename { name },
            bae_core::library::UploadFileLabel::Cover => Self::Cover,
            bae_core::library::UploadFileLabel::ArtistImage => Self::ArtistImage,
        }
    }
}

impl crate::types::BridgeDeleteOp {
    pub(super) fn from_core(op: bae_core::library::DeleteOp) -> Self {
        let bae_core::library::DeleteOp {
            namespace,
            blob_id,
            created_at,
        } = op;
        Self {
            namespace,
            blob_id,
            created_at,
        }
    }
}

impl crate::types::BridgeOutboxSnapshot {
    pub(super) fn from_core(snapshot: bae_core::library::OutboxSnapshot) -> Self {
        // Derived aggregates borrow `&snapshot`; compute them before the move.
        let per_release = snapshot
            .per_release_progress()
            .into_iter()
            .map(|(release_id, progress)| {
                (
                    release_id,
                    crate::types::BridgeUploadProgress::from_core(progress),
                )
            })
            .collect();
        let pending_deletes = snapshot.pending_delete_count();
        let summary_parts = snapshot
            .summary_parts()
            .into_iter()
            .map(crate::types::BridgeCountLabel::from_core)
            .collect();

        let bae_core::library::OutboxSnapshot {
            revision,
            upload_groups,
            deletes,
            total,
            pause_state,
            throughput_bps,
            eta_seconds,
        } = snapshot;

        crate::types::BridgeOutboxSnapshot {
            revision,
            upload_groups: upload_groups
                .into_iter()
                .map(crate::types::BridgeUploadReleaseGroup::from_core)
                .collect(),
            deletes: deletes
                .into_iter()
                .map(crate::types::BridgeDeleteOp::from_core)
                .collect(),
            per_release,
            total: crate::types::BridgeUploadProgress::from_core(total),
            pending_deletes,
            summary_parts,
            pause_state: crate::types::BridgeOutboxPauseState::from_core(pause_state),
            throughput_bps,
            eta_seconds,
        }
    }
}

impl crate::types::BridgeOutboxPauseState {
    fn from_core(state: bae_core::library::OutboxPauseState) -> Self {
        match state {
            bae_core::library::OutboxPauseState::Running => Self::Running,
            bae_core::library::OutboxPauseState::Pausing => Self::Pausing,
            bae_core::library::OutboxPauseState::Paused => Self::Paused,
        }
    }
}

impl crate::types::BridgeUploadProgress {
    pub(super) fn from_core(p: bae_core::library::UploadProgress) -> Self {
        // `activity()` borrows `&p`; compute it before destructuring `p`.
        let activity = p
            .activity()
            .map(crate::types::BridgeUploadActivity::from_core);
        let bae_core::library::UploadProgress {
            queued,
            preparing,
            prepared,
            uploading,
            failed,
            uploaded,
            publishing,
            cancelling,
            preparation_bytes_done,
            preparation_bytes_total,
            upload_bytes_done,
            upload_bytes_total,
            upload_bytes_total_complete,
            work_done,
            work_total,
        } = p;
        crate::types::BridgeUploadProgress {
            queued,
            preparing,
            prepared,
            uploading,
            failed,
            uploaded,
            publishing,
            cancelling,
            preparation_bytes_done,
            preparation_bytes_total,
            upload_bytes_done,
            upload_bytes_total,
            upload_bytes_total_complete,
            work_done,
            work_total,
            activity,
        }
    }
}

impl crate::types::BridgeUploadActivity {
    pub(super) fn from_core(a: bae_core::library::UploadActivity) -> Self {
        use crate::types::BridgeUploadActivity;
        use bae_core::library::UploadActivity;
        match a {
            UploadActivity::Cancelling => BridgeUploadActivity::Cancelling,
            UploadActivity::Publishing => BridgeUploadActivity::Publishing,
            UploadActivity::Uploading => BridgeUploadActivity::Uploading,
            UploadActivity::Preparing => BridgeUploadActivity::Preparing,
            UploadActivity::Retrying => BridgeUploadActivity::Retrying,
            UploadActivity::Prepared => BridgeUploadActivity::Prepared,
            UploadActivity::Queued => BridgeUploadActivity::Queued,
            UploadActivity::Uploaded => BridgeUploadActivity::Uploaded,
        }
    }
}

impl crate::types::BridgeDownloadTransferProgress {
    pub(crate) fn from_core(p: bae_core::library::DownloadTransferProgress) -> Self {
        let bae_core::library::DownloadTransferProgress {
            bytes_done,
            bytes_total,
            fraction,
        } = p;
        Self {
            bytes_done,
            bytes_total,
            fraction,
        }
    }
}

impl crate::types::BridgeDownloadState {
    pub(super) fn from_core(state: bae_core::library::DownloadState) -> Self {
        use crate::types::BridgeDownloadState;
        use bae_core::library::DownloadState;
        match state {
            DownloadState::Queued => BridgeDownloadState::Queued,
            DownloadState::Active { progress } => BridgeDownloadState::Active {
                progress: crate::types::BridgeDownloadTransferProgress::from_core(progress),
            },
            DownloadState::Failed { error } => BridgeDownloadState::Failed { error },
        }
    }
}

impl crate::types::BridgeDownloadOp {
    pub(super) fn from_core(op: bae_core::library::DownloadOp) -> Self {
        let bae_core::library::release_queue::ReleaseQueueOp {
            release_id,
            title,
            file_count,
            total_size,
            created_at,
            // Downloads carry no operation-specific payload.
            payload: (),
            state,
        } = op;
        crate::types::BridgeDownloadOp {
            release_id,
            title,
            file_count,
            total_size,
            created_at,
            state: crate::types::BridgeDownloadState::from_core(state),
        }
    }
}

/// Shared projection for the download and export queue snapshots, which are both
/// aliases of the same generic `ReleaseQueueSnapshot`. Parameterized over the
/// per-op and per-progress converters so each snapshot keeps its own named fields.
fn project_release_queue_snapshot<Extra, Progress, Op, Prog>(
    snapshot: bae_core::library::release_queue::ReleaseQueueSnapshot<Extra, Progress>,
    op_from_core: impl Fn(bae_core::library::release_queue::ReleaseQueueOp<Extra, Progress>) -> Op,
    progress_from_core: impl Fn(bae_core::library::release_queue::ReleaseQueueProgress) -> Prog,
) -> (Vec<Op>, Prog, bool) {
    let bae_core::library::release_queue::ReleaseQueueSnapshot { ops, total, paused } = snapshot;
    (
        ops.into_iter().map(op_from_core).collect(),
        progress_from_core(total),
        paused,
    )
}

/// Shared projection for the download and export per-state counts, both aliases
/// of the same generic `ReleaseQueueProgress`.
fn release_queue_progress_counts(
    p: bae_core::library::release_queue::ReleaseQueueProgress,
) -> (u32, u32, u32) {
    let bae_core::library::release_queue::ReleaseQueueProgress {
        queued,
        active,
        failed,
    } = p;
    (queued, active, failed)
}

impl crate::types::BridgeDownloadSnapshot {
    pub(super) fn from_core(snapshot: bae_core::library::DownloadSnapshot) -> Self {
        let summary_parts = snapshot
            .total
            .summary_parts("core.queue.downloading")
            .into_iter()
            .map(crate::types::BridgeCountLabel::from_core)
            .collect();
        let (downloads, total, paused) = project_release_queue_snapshot(
            snapshot,
            crate::types::BridgeDownloadOp::from_core,
            crate::types::BridgeDownloadProgress::from_core,
        );
        crate::types::BridgeDownloadSnapshot {
            downloads,
            total,
            summary_parts,
            paused,
        }
    }
}

impl crate::types::BridgeDownloadProgress {
    pub(super) fn from_core(p: bae_core::library::DownloadProgress) -> Self {
        let (queued, active, failed) = release_queue_progress_counts(p);
        crate::types::BridgeDownloadProgress {
            queued,
            active,
            failed,
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl crate::types::BridgeOutputState {
    pub(super) fn from_core(state: bae_core::library::OutputState) -> Self {
        use crate::types::BridgeOutputState;
        use bae_core::library::OutputState;
        match state {
            OutputState::Queued => BridgeOutputState::Queued,
            OutputState::Active { progress } => BridgeOutputState::Active { percent: progress },
            OutputState::Failed { error } => BridgeOutputState::Failed { error },
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl crate::types::BridgeOutputOp {
    pub(super) fn from_core(op: bae_core::library::OutputOp) -> Self {
        let bae_core::library::release_queue::ReleaseQueueOp {
            release_id,
            title,
            file_count,
            total_size,
            created_at,
            payload,
            state,
        } = op;
        let bae_core::library::output_snapshot::OutputRequest { target_dir, kind } = payload;
        crate::types::BridgeOutputOp {
            release_id,
            target_dir: target_dir.to_string_lossy().to_string(),
            title,
            file_count,
            total_size,
            created_at,
            state: crate::types::BridgeOutputState::from_core(state),
            kind: crate::types::BridgeOutputKind::from_core(&kind),
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl crate::types::BridgeOutputSnapshot {
    pub(super) fn from_core(snapshot: bae_core::library::OutputSnapshot) -> Self {
        let summary_parts = snapshot
            .total
            .summary_parts("core.queue.output")
            .into_iter()
            .map(crate::types::BridgeCountLabel::from_core)
            .collect();
        let (outputs, total, paused) = project_release_queue_snapshot(
            snapshot,
            crate::types::BridgeOutputOp::from_core,
            crate::types::BridgeOutputProgress::from_core,
        );
        crate::types::BridgeOutputSnapshot {
            outputs,
            total,
            summary_parts,
            paused,
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl crate::types::BridgeOutputProgress {
    pub(super) fn from_core(p: bae_core::library::OutputProgress) -> Self {
        let (queued, active, failed) = release_queue_progress_counts(p);
        crate::types::BridgeOutputProgress {
            queued,
            active,
            failed,
        }
    }
}

impl crate::types::BridgeQueueEntry {
    pub(super) fn from_core(i: bae_core::queue::QueueItem) -> Self {
        let bae_core::queue::QueueItem {
            entry_id,
            track_id,
            title,
            artist_names,
            duration_ms,
            album_title,
            cover_image,
        } = i;
        crate::types::BridgeQueueEntry {
            entry_id,
            track_id,
            title,
            artist_names,
            duration_clock: crate::types::BridgeDurationClock::from_millis(duration_ms),
            album_title,
            cover_image: cover_image.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl crate::types::BridgePlaybackContext {
    pub(super) fn from_core(context: bae_core::queue::ResolvedContext) -> Self {
        let bae_core::queue::ResolvedContext {
            source,
            source_title,
            shuffled,
            upcoming,
            upcoming_total,
        } = context;
        crate::types::BridgePlaybackContext {
            kind: crate::types::BridgePlaybackSourceKind::from_core(&source),
            source_title,
            shuffled,
            upcoming: upcoming
                .into_iter()
                .map(crate::types::BridgeQueueEntry::from_core)
                .collect(),
            upcoming_total,
        }
    }
}

impl crate::types::BridgeQueueSnapshot {
    pub(super) fn from_core(snapshot: bae_core::queue::ResolvedQueueSnapshot) -> Self {
        let bae_core::queue::ResolvedQueueSnapshot {
            manual,
            context,
            has_next,
            has_previous,
            revision,
        } = snapshot;
        crate::types::BridgeQueueSnapshot {
            manual: manual
                .into_iter()
                .map(crate::types::BridgeQueueEntry::from_core)
                .collect(),
            context: context.map(crate::types::BridgePlaybackContext::from_core),
            has_next,
            has_previous,
            revision,
        }
    }
}

impl crate::types::BridgeQueueUpcomingPage {
    pub(super) fn from_core(page: bae_core::queue::ResolvedQueueUpcomingPage) -> Self {
        let bae_core::queue::ResolvedQueueUpcomingPage { revision, items } = page;
        crate::types::BridgeQueueUpcomingPage {
            revision,
            entries: items
                .into_iter()
                .map(crate::types::BridgeQueueEntry::from_core)
                .collect(),
        }
    }
}

impl crate::types::BridgeSyncStatusSnapshot {
    pub(super) fn from_core(snapshot: bae_core::library::SyncStatusSnapshot) -> Self {
        let bae_core::library::SyncStatusSnapshot {
            error,
            last_sync_time,
            syncing,
            sync_ready,
        } = snapshot;
        crate::types::BridgeSyncStatusSnapshot {
            error: error.map(crate::types::BridgeError::from_core),
            last_sync_time,
            syncing,
            sync_ready,
        }
    }
}
