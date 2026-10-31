mirror_struct! {
    crate::types::BridgeUploadBar = bae_core::library::UploadBar,
    from_core: pub(super) fn,
    fields: { bytes_done, bytes_total },
}

mirror_struct! {
    crate::types::BridgeUploadTransfer = bae_core::library::UploadTransfer,
    from_core: pub(super) fn,
    fields: { bar: (opt crate::types::BridgeUploadBar), throughput_bps },
}

mirror_struct! {
    crate::types::BridgeReleaseUploadTransfer = bae_core::library::ReleaseUploadTransfer,
    from_core: pub(super) fn,
    fields: { bar: (crate::types::BridgeUploadBar), throughput_bps },
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
    fields: { bytes_done, bytes_total, fraction, throughput_bps },
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

impl crate::types::BridgeDownloadCounts {
    pub(super) fn from_core(counts: bae_core::library::ReleaseQueueCounts) -> Self {
        let summary_parts = counts
            .total
            .summary_parts("core.queue.downloading")
            .into_iter()
            .map(crate::types::BridgeCountLabel::from_core)
            .collect();
        let bae_core::library::ReleaseQueueCounts { total, paused } = counts;
        crate::types::BridgeDownloadCounts {
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

#[cfg(desktop)]
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

#[cfg(desktop)]
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
            target_dir: coven::path_text(&target_dir),
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
        } = context;
        crate::types::BridgePlaybackContext {
            kind: crate::types::BridgePlaybackSourceKind::from_core(&source),
            source_title,
            shuffled,
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
    },
}

impl crate::types::BridgeSyncStatusSnapshot {
    pub(super) fn from_core(snapshot: bae_core::library::SyncStatusSnapshot) -> Self {
        let indicator = crate::types::BridgeSyncIndicator::from_core(snapshot.indicator());
        let error = snapshot.shown_failure();
        let connected = snapshot.connected();
        let bae_core::library::SyncStatusSnapshot {
            status: _,
            pending,
            error: _,
            retry_effect: _,
        } = snapshot;
        Self {
            indicator,
            connected,
            error: error.map(crate::types::BridgeError::from_core),
            pending: crate::types::BridgePendingRecords::from_core(pending),
        }
    }
}
