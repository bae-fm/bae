#[cfg(not(any(target_os = "ios", target_os = "android")))]
mirror_struct! {
    crate::types::BridgeUploadReleaseGroup = bae_core::library::UploadReleaseGroup,
    from_core: pub(super) fn,
    fields: {
        release_id,
        files: (each crate::types::BridgeUploadFileOp),
        progress: (crate::types::BridgeUploadProgress),
    },
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl crate::types::BridgeUploadFileOp {
    pub(super) fn from_core(file: bae_core::library::UploadFileOp) -> Self {
        let bae_core::library::UploadFileOp {
            file,
            phase,
            pending,
            label,
        } = file;
        Self {
            pending: pending.map(crate::types::BridgePendingWait::from_core),
            file_id: file.id().to_string(),
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
mirror_enum! {
    crate::types::BridgeUploadFileLabel = bae_core::library::UploadFileLabel,
    from_core: fn,
    variants: { Filename(name), Cover, ArtistImage, Avatar, DeletedReleaseFile },
}

mirror_struct! {
    crate::types::BridgeUploadTransfer = bae_core::library::UploadTransfer,
    from_core: pub(super) fn,
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
    pub(super) fn from_core(summary: bae_core::library::OutboxSummary) -> Self {
        let bae_core::library::OutboxSummary {
            revision,
            per_release,
            pause_state,
        } = summary;
        crate::types::BridgeOutboxSnapshot {
            revision,
            per_release: per_release
                .into_iter()
                .map(|(release_id, progress)| {
                    (
                        release_id,
                        crate::types::BridgeUploadProgress::from_core(progress),
                    )
                })
                .collect(),
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
    fields: { bytes_done, bytes_total, fraction },
}

impl crate::types::BridgeDownloadState {
    pub(super) fn from_core(state: bae_core::library::DownloadPhase) -> Self {
        match state {
            bae_core::library::DownloadPhase::Queued => Self::Queued,
            bae_core::library::DownloadPhase::Active { progress: () } => Self::Active,
            bae_core::library::DownloadPhase::Failed { error } => Self::Failed { error },
        }
    }
}

impl crate::types::BridgeDownloadTransferChange {
    pub(super) fn from_core(change: bae_core::library::DownloadTransferChange) -> Self {
        use bae_core::library::ReleaseTransferChange;
        match change {
            ReleaseTransferChange::Updated {
                release_id,
                progress,
            } => Self::Updated {
                release_id,
                progress: crate::types::BridgeDownloadTransferProgress::from_core(progress),
            },
            ReleaseTransferChange::Removed { release_id } => Self::Removed { release_id },
            ReleaseTransferChange::Reset(transfers) => Self::Reset {
                transfers: transfers
                    .into_iter()
                    .map(
                        |(release_id, progress)| crate::types::BridgeKeyedDownloadTransfer {
                            release_id,
                            progress: crate::types::BridgeDownloadTransferProgress::from_core(
                                progress,
                            ),
                        },
                    )
                    .collect(),
            },
        }
    }
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

impl crate::types::BridgeDownloadSnapshot {
    pub(super) fn from_core(snapshot: bae_core::library::DownloadSnapshot) -> Self {
        let summary_parts = snapshot
            .total
            .summary_parts("core.queue.downloading")
            .into_iter()
            .map(crate::types::BridgeCountLabel::from_core)
            .collect();
        let bae_core::library::release_queue::ReleaseQueueSnapshot {
            rows,
            total,
            paused,
        } = snapshot;
        crate::types::BridgeDownloadSnapshot {
            downloads: rows
                .into_iter()
                .map(crate::types::BridgeDownloadOp::from_core)
                .collect(),
            total: crate::types::BridgeDownloadProgress::from_core(total),
            summary_parts,
            paused,
        }
    }
}

impl crate::types::BridgeDownloadProgress {
    pub(super) fn from_core(p: bae_core::library::DownloadProgress) -> Self {
        let bae_core::library::release_queue::ReleaseQueueProgress {
            queued,
            active,
            failed,
        } = p;
        crate::types::BridgeDownloadProgress {
            queued,
            active,
            failed,
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl crate::types::BridgeOutputState {
    pub(super) fn from_core(state: bae_core::library::OutputPhase) -> Self {
        use crate::types::BridgeOutputState;
        use bae_core::library::OutputPhase;
        match state {
            OutputPhase::Queued => BridgeOutputState::Queued,
            OutputPhase::Active { progress: () } => BridgeOutputState::Active,
            OutputPhase::Failed { error } => BridgeOutputState::Failed { error },
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl crate::types::BridgeOutputTransferChange {
    pub(super) fn from_core(change: bae_core::library::OutputTransferChange) -> Self {
        use bae_core::library::ReleaseTransferChange;
        match change {
            ReleaseTransferChange::Updated {
                release_id,
                progress,
            } => Self::Updated {
                release_id,
                percent: progress,
            },
            ReleaseTransferChange::Removed { release_id } => Self::Removed { release_id },
            ReleaseTransferChange::Reset(transfers) => Self::Reset {
                transfers: transfers
                    .into_iter()
                    .map(
                        |(release_id, percent)| crate::types::BridgeKeyedOutputTransfer {
                            release_id,
                            percent,
                        },
                    )
                    .collect(),
            },
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl crate::types::BridgeOutputOp {
    pub(super) fn from_core(row: bae_core::library::OutputPhaseRow) -> Self {
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
