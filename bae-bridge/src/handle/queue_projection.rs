mirror_struct! {
    crate::types::BridgeUploadReleaseGroup = bae_core::library::UploadReleaseGroup,
    from_core: pub(super) fn,
    fields: {
        release_id,
        files: (each crate::types::BridgeUploadFileOp),
        progress: (crate::types::BridgeUploadProgress),
    },
}

impl crate::types::BridgeUploadFileOp {
    pub(super) fn from_core(file: bae_core::library::UploadFileOp) -> Self {
        let release_file_key = file
            .release_file_key()
            .cloned()
            .map(crate::types::BridgeRowKey::from_core);
        let bae_core::library::UploadFileOp {
            file,
            phase,
            pending,
            label,
        } = file;
        Self {
            pending: pending.map(crate::types::BridgePendingWait::from_core),
            file_id: file.id().to_string(),
            release_file_key,
            label: crate::types::BridgeUploadFileLabel::from_core(label),
            state: match phase {
                coven::UploadPhase::Waiting => crate::types::BridgeUploadFileState::Waiting,
                coven::UploadPhase::Uploading => crate::types::BridgeUploadFileState::Uploading,
            },
        }
    }
}
mirror_struct! {
    crate::types::BridgeUploadBar = bae_core::library::UploadBar,
    from_core: fn,
    fields: { bytes_done, bytes_total },
}

mirror_enum! {
    crate::types::BridgeUploadFileLabel = bae_core::library::UploadFileLabel,
    from_core: fn,
    variants: { Filename(name), Cover, ArtistImage, Avatar, DeletedReleaseFile },
}

mirror_struct! {
    crate::types::BridgeUploadTransfer = bae_core::library::UploadTransfer,
    from_core: fn,
    fields: { bar: (opt crate::types::BridgeUploadBar), throughput_bps },
}

impl crate::types::BridgeUploadTransferKey {
    fn from_core(key: bae_core::library::UploadTransferKey) -> Self {
        match key {
            bae_core::library::UploadTransferKey::File(file_id) => Self::File {
                file_id: file_id.to_string(),
            },
            bae_core::library::UploadTransferKey::Release(release_id) => {
                Self::Release { release_id }
            }
        }
    }
}

impl crate::types::BridgeOutboxTransferChange {
    pub(super) fn from_core(change: bae_core::library::OutboxTransferChange) -> Self {
        use bae_core::library::OutboxTransferChange;
        match change {
            OutboxTransferChange::Updated { key, transfer } => Self::Updated {
                key: crate::types::BridgeUploadTransferKey::from_core(key),
                transfer: crate::types::BridgeUploadTransfer::from_core(transfer),
            },
            OutboxTransferChange::Removed { key } => Self::Removed {
                key: crate::types::BridgeUploadTransferKey::from_core(key),
            },
            OutboxTransferChange::TotalUpdated(transfer) => Self::TotalUpdated {
                transfer: crate::types::BridgeUploadTransfer::from_core(transfer),
            },
            OutboxTransferChange::Reset(transfers) => Self::Reset {
                transfers: transfers
                    .keyed
                    .into_iter()
                    .map(|(key, transfer)| crate::types::BridgeKeyedUploadTransfer {
                        key: crate::types::BridgeUploadTransferKey::from_core(key),
                        transfer: crate::types::BridgeUploadTransfer::from_core(transfer),
                    })
                    .collect(),
                total: crate::types::BridgeUploadTransfer::from_core(transfers.total),
            },
        }
    }
}

impl crate::types::BridgeOutboxSnapshot {
    /// The queue `state` holds, at its revision.
    pub(super) fn from_core(state: bae_core::library::OutboxState) -> Self {
        let bae_core::library::OutboxState {
            revision, queue, ..
        } = state;
        // Derived aggregates borrow `&queue`; compute them before the move.
        let per_release = queue
            .per_release_progress()
            .into_iter()
            .map(|(release_id, progress)| {
                (
                    release_id,
                    crate::types::BridgeUploadProgress::from_core(progress),
                )
            })
            .collect();
        let summary_parts = queue
            .summary_parts()
            .into_iter()
            .map(crate::types::BridgeCountLabel::from_core)
            .collect();

        let bae_core::library::OutboxQueue {
            upload_groups,
            other_files,
            total,
            pause_state,
        } = queue;

        crate::types::BridgeOutboxSnapshot {
            revision,
            upload_groups: upload_groups
                .into_iter()
                .map(crate::types::BridgeUploadReleaseGroup::from_core)
                .collect(),
            per_release,
            other_files: other_files
                .into_iter()
                .map(crate::types::BridgeUploadFileOp::from_core)
                .collect(),
            total: crate::types::BridgeUploadProgress::from_core(total),
            summary_parts,
            pause_state: crate::types::BridgeOutboxPauseState::from_core(pause_state),
        }
    }
}

mirror_enum! {
    crate::types::BridgeOutboxPauseState = bae_core::library::OutboxPauseState,
    from_core: fn,
    variants: { Running, Paused },
}

impl crate::types::BridgeUploadProgress {
    pub(super) fn from_core(progress: bae_core::library::UploadProgress) -> Self {
        let activity = progress
            .activity()
            .map(crate::types::BridgeUploadActivity::from_core);
        let bae_core::library::UploadProgress { queued, uploading } = progress;
        Self {
            queued,
            uploading,
            activity,
        }
    }
}
mirror_enum! {
    crate::types::BridgeUploadActivity = bae_core::library::UploadActivity,
    from_core: pub(super) fn,
    variants: { Uploading, Queued },
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
    pub(super) fn from_core(row: bae_core::library::DownloadRow) -> Self {
        let bae_core::library::release_queue::ReleaseQueueRow { op, release } = row;
        let bae_core::library::release_queue::ReleaseQueueOp {
            release_id,
            created_at,
            // Downloads carry no operation-specific payload.
            payload: (),
            state,
        } = op;
        crate::types::BridgeDownloadOp {
            release_id,
            release: release.map(crate::types::BridgeQueuedRelease::from_core),
            created_at,
            state: crate::types::BridgeDownloadState::from_core(state),
        }
    }
}

/// Shared projection for the download and export queue snapshots, which are both
/// aliases of the same generic `ReleaseQueueSnapshot`. Parameterized over the
/// per-row and per-progress converters so each snapshot keeps its own named fields.
fn project_release_queue_snapshot<Extra, Progress, Op, Prog>(
    snapshot: bae_core::library::release_queue::ReleaseQueueSnapshot<Extra, Progress>,
    row_from_core: impl Fn(bae_core::library::release_queue::ReleaseQueueRow<Extra, Progress>) -> Op,
    progress_from_core: impl Fn(bae_core::library::release_queue::ReleaseQueueProgress) -> Prog,
) -> (Vec<Op>, Prog, bool) {
    let bae_core::library::release_queue::ReleaseQueueSnapshot {
        rows,
        total,
        paused,
    } = snapshot;
    (
        rows.into_iter().map(row_from_core).collect(),
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
    pub(super) fn from_core(row: bae_core::library::OutputRow) -> Self {
        let bae_core::library::release_queue::ReleaseQueueRow { op, release } = row;
        let bae_core::library::release_queue::ReleaseQueueOp {
            release_id,
            created_at,
            payload,
            state,
        } = op;
        let bae_core::library::output_snapshot::OutputRequest { target_dir, kind } = payload;
        crate::types::BridgeOutputOp {
            release_id,
            target_dir: target_dir.to_string_lossy().to_string(),
            release: release.map(crate::types::BridgeQueuedRelease::from_core),
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
            display,
            duration_ms,
        } = i;
        crate::types::BridgeQueueEntry {
            entry_id,
            track_id,
            display: crate::types::BridgeTrackDisplay::from_core(display),
            duration_clock: crate::types::BridgeDurationClock::of_length(duration_ms),
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

impl crate::types::BridgeSyncStatusSnapshot {
    pub(super) fn from_core(snapshot: bae_core::library::SyncStatusSnapshot) -> Self {
        let indicator = crate::types::BridgeSyncIndicator::from_core(snapshot.indicator());
        let error = snapshot.failure();
        let connected = snapshot.connected();
        let bae_core::library::SyncStatusSnapshot {
            status: _,
            pending,
            abandoned: _,
            error: _,
        } = snapshot;
        Self {
            indicator,
            connected,
            error: error.map(crate::types::BridgeError::from_core),
            pending: crate::types::BridgePendingRecords::from_core(pending),
        }
    }
}
