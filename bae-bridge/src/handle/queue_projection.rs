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

mirror_struct! {
    crate::types::BridgeUploadReleaseGroup = bae_core::library::UploadReleaseGroup,
    from_core: pub(super) fn,
    fields: {
        release_id,
        display_title,
        files: (each crate::types::BridgeUploadFileOp),
        progress: (crate::types::BridgeUploadProgress),
        throughput_bps,
    },
}

impl crate::types::BridgeUploadFileOp {
    /// Flatten core's per-file `UploadState` into `state` + `bar` +
    /// `last_error`, so the UI reads plain fields instead of switching on
    /// associated data.
    pub(super) fn from_core(f: bae_core::library::UploadFileOp) -> Self {
        use bae_core::library::UploadState;
        let bae_core::library::UploadFileOp {
            file_id,
            label,
            source_bytes_total,
            throughput_bps,
            state,
        } = f;
        let bar = state.bar().map(crate::types::BridgeUploadBar::from_core);
        let (state, last_error) = match state {
            UploadState::Queued => (crate::types::BridgeUploadFileState::Queued, None),
            UploadState::Preparing { .. } => (crate::types::BridgeUploadFileState::Preparing, None),
            UploadState::Prepared { .. } => (crate::types::BridgeUploadFileState::Prepared, None),
            UploadState::Uploading { .. } => (crate::types::BridgeUploadFileState::Uploading, None),
            UploadState::RetryingPreparation { last_error }
            | UploadState::RetryingUpload { last_error, .. }
            | UploadState::RetryingPublication { last_error, .. } => (
                crate::types::BridgeUploadFileState::Retrying,
                Some(last_error),
            ),
            UploadState::Uploaded { .. } => (crate::types::BridgeUploadFileState::Uploaded, None),
        };
        Self {
            file_id,
            label: crate::types::BridgeUploadFileLabel::from_core(label),
            bar,
            source_bytes_total,
            throughput_bps,
            state,
            last_error,
        }
    }
}

mirror_struct! {
    crate::types::BridgeUploadBar = bae_core::library::UploadBar,
    from_core: fn,
    fields: {
        phase: (crate::types::BridgeUploadPhase),
        bytes_done,
        bytes_total,
    },
}

mirror_enum! {
    crate::types::BridgeUploadPhase = bae_core::library::UploadPhase,
    from_core: fn,
    variants: { Preparing, Uploading },
}

mirror_enum! {
    crate::types::BridgeUploadFileLabel = bae_core::library::UploadFileLabel,
    from_core: fn,
    variants: { Filename(name), Cover, ArtistImage, Unwinding },
}

mirror_struct! {
    crate::types::BridgeDeleteOp = bae_core::library::DeleteOp,
    from_core: pub(super) fn,
    fields: { namespace, blob_id, created_at },
}

impl crate::types::BridgeOutboxSnapshot {
    pub(super) fn from_core(snapshot: bae_core::library::OutboxSnapshot) -> Self {
        // Derived aggregates borrow `&snapshot`; compute them before the move.
        let per_release = snapshot
            .per_release_progress()
            .into_iter()
            .map(|(release_id, release_progress)| {
                (
                    release_id,
                    crate::types::BridgeReleaseUploadProgress {
                        progress: crate::types::BridgeUploadProgress::from_core(
                            release_progress.progress,
                        ),
                        throughput_bps: release_progress.throughput_bps,
                    },
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

mirror_enum! {
    crate::types::BridgeOutboxPauseState = bae_core::library::OutboxPauseState,
    from_core: fn,
    variants: { Running, Paused },
}

impl crate::types::BridgeUploadProgress {
    pub(super) fn from_core(p: bae_core::library::UploadProgress) -> Self {
        // Derived fields borrow `&p`; compute them before destructuring `p`.
        let activity = p
            .activity()
            .map(crate::types::BridgeUploadActivity::from_core);
        let can_cancel = p.can_cancel();
        let bar = p.bar().map(crate::types::BridgeUploadBar::from_core);
        let bae_core::library::UploadProgress {
            queued,
            preparing,
            prepared,
            uploading,
            retrying,
            uploaded,
            publishing,
            cancelling,
            preparation_bytes_done: _,
            preparation_bytes_total: _,
            upload_bytes_done: _,
            upload_bytes_total: _,
            // The phase-scoped `bar` is what the UI draws and labels; the raw
            // per-phase byte sums it derives from stay in core.
            upload_bytes_total_complete: _,
            issue,
        } = p;
        crate::types::BridgeUploadProgress {
            queued,
            preparing,
            prepared,
            uploading,
            retrying,
            uploaded,
            publishing,
            cancelling,
            bar,
            activity,
            can_cancel,
            issue: issue.map(crate::types::BridgeUploadIssue::from_core),
        }
    }
}

impl crate::types::BridgeUploadIssue {
    fn from_core(issue: bae_core::library::UploadIssue) -> Self {
        match issue {
            bae_core::library::UploadIssue::SourceUnavailable { paths } => {
                Self::SourceUnavailable {
                    paths: paths
                        .into_iter()
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect(),
                }
            }
        }
    }
}

mirror_enum! {
    crate::types::BridgeUploadActivity = bae_core::library::UploadActivity,
    from_core: pub(super) fn,
    variants: {
        Cancelling,
        Publishing,
        Uploading,
        Preparing,
        Retrying,
        Prepared,
        Queued,
        Uploaded,
    },
}

mirror_struct! {
    crate::types::BridgeDownloadTransferProgress = bae_core::library::DownloadTransferProgress,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: { bytes_done, bytes_total, fraction },
}

mirror_enum! {
    crate::types::BridgeDownloadState = bae_core::library::DownloadState,
    from_core: pub(super) fn,
    into_core: pub(crate) fn,
    variants: {
        Queued,
        Active { progress: (crate::types::BridgeDownloadTransferProgress) },
        Failed { error },
    },
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

mirror_struct! {
    crate::types::BridgeQueueSnapshot = bae_core::queue::ResolvedQueueSnapshot,
    from_core: pub(super) fn,
    fields: {
        manual: (each crate::types::BridgeQueueEntry),
        context: (opt crate::types::BridgePlaybackContext),
        has_next,
        has_previous,
        revision,
    },
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
        let can_reconnect = snapshot
            .error
            .as_ref()
            .is_some_and(bae_core::ui::UiError::can_reconnect_sync);
        let bae_core::library::SyncStatusSnapshot {
            error,
            blocked,
            last_sync_time,
            syncing,
            sync_ready,
        } = snapshot;
        crate::types::BridgeSyncStatusSnapshot {
            error: error.map(crate::types::BridgeError::from_core),
            can_reconnect,
            blocked: blocked
                .into_iter()
                .map(crate::types::BridgeBlockedSyncOperation::from_core)
                .collect(),
            last_sync_time,
            syncing,
            sync_ready,
        }
    }
}
