mirror_struct! {
    crate::types::BridgeUploadBar = bae_library::UploadBar,
    from_core: pub(super) fn,
    fields: { bytes_done, bytes_total },
}

mirror_struct! {
    crate::types::BridgeUploadTransfer = bae_library::UploadTransfer,
    from_core: pub(crate) fn,
    fields: { bar: (opt crate::types::BridgeUploadBar), throughput_bps },
}

mirror_struct! {
    crate::types::BridgeReleaseUploadTransfer = bae_library::ReleaseUploadTransfer,
    from_core: pub(crate) fn,
    fields: { bar: (crate::types::BridgeUploadBar), throughput_bps },
}

impl crate::types::BridgeUploadProgress {
    pub(super) fn from_core(progress: bae_records::UploadProgress) -> Self {
        let activity = crate::types::BridgeUploadActivity::from_core(progress.activity());
        let bae_records::UploadProgress { queued, uploading } = progress;
        Self { queued, uploading, activity }
    }
}
mirror_enum! {
    crate::types::BridgeUploadActivity = bae_records::UploadActivity,
    from_core: pub(super) fn,
    variants: { Uploading, Queued },
}

mirror_struct! {
    crate::types::BridgeDownloadTransferProgress = bae_transfers::DownloadTransferProgress,
    from_core: pub(crate) fn,
    fields: { bytes_done, bytes_total, fraction, throughput_bps },
}

mirror_struct! {
    crate::types::BridgeMakeLocalProgress = bae_transfers::MakeLocalTransferProgress,
    from_core: pub(crate) fn,
    fields: { file_bytes_done, file_bytes_total, bytes_done, bytes_total },
}

impl crate::types::BridgeDownloadState {
    pub(super) fn from_core(state: bae_transfers::DownloadPhase) -> Self {
        match state {
            bae_transfers::DownloadPhase::Queued => Self::Queued,
            bae_transfers::DownloadPhase::Active { progress: () } => Self::Active,
            bae_transfers::DownloadPhase::Failed { error } => Self::Failed { error },
        }
    }
}

impl crate::types::BridgeDownloadOp {
    pub(crate) fn from_core(row: bae_transfers::DownloadRow) -> Self {
        let bae_transfers::ReleaseQueueRow { op, release } = row;
        let bae_transfers::ReleaseQueueOp { release_id, created_at, payload, state } = op;
        let kind = match payload {
            bae_transfers::DownloadRequest::Pin => crate::types::BridgeDownloadKind::Pin,
            #[cfg(desktop)]
            bae_transfers::DownloadRequest::MakeLocal { .. } => {
                crate::types::BridgeDownloadKind::MakeLocal
            }
        };
        crate::types::BridgeDownloadOp {
            release_id,
            kind,
            release: release.map(crate::types::BridgeQueuedRelease::from_core),
            created_at,
            state: crate::types::BridgeDownloadState::from_core(state),
        }
    }
}

impl crate::types::BridgeDownloadCounts {
    pub(crate) fn from_core(counts: bae_transfers::ReleaseQueueCounts) -> Self {
        let bae_transfers::ReleaseQueueCounts { total, wait } = counts;
        crate::types::BridgeDownloadCounts {
            total: crate::types::BridgeDownloadProgress::from_core(total),
            wait: wait.map(crate::types::BridgeQueueWait::from_core),
        }
    }
}

impl crate::types::BridgeDownloadProgress {
    pub(super) fn from_core(p: bae_transfers::DownloadProgress) -> Self {
        let bae_transfers::ReleaseQueueProgress { queued, active, failed } = p;
        crate::types::BridgeDownloadProgress { queued, active, failed }
    }
}

#[cfg(desktop)]
impl crate::types::BridgeOutputState {
    pub(super) fn from_core(state: bae_transfers::OutputPhase) -> Self {
        use crate::types::BridgeOutputState;
        use bae_transfers::OutputPhase;
        match state {
            OutputPhase::Queued => BridgeOutputState::Queued,
            OutputPhase::Active { progress: () } => BridgeOutputState::Active,
            OutputPhase::Failed { error } => BridgeOutputState::Failed { error },
        }
    }
}

#[cfg(desktop)]
impl crate::types::BridgeOutputOp {
    pub(super) fn from_core(row: bae_transfers::OutputPhaseRow) -> Self {
        let bae_transfers::ReleaseQueueRow { op, release } = row;
        let bae_transfers::ReleaseQueueOp { release_id, created_at, payload, state } = op;
        let bae_transfers::OutputRequest { target_dir, kind } = payload;
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
    pub(crate) fn from_core(i: bae_playback::QueueItem) -> Self {
        let bae_playback::QueueItem { entry_id, track_id, display, duration_ms } = i;
        crate::types::BridgeQueueEntry {
            entry_id,
            track_id,
            display: crate::types::BridgeTrackDisplay::from_core(display),
            duration_clock: crate::types::BridgeDurationClock::of_length(duration_ms),
        }
    }
}

impl crate::types::BridgePlaybackContext {
    pub(super) fn from_core(context: bae_playback::ResolvedContext) -> Self {
        let bae_playback::ResolvedContext { source, source_title, shuffled } = context;
        crate::types::BridgePlaybackContext {
            kind: crate::types::BridgePlaybackSourceKind::from_core(&source),
            source_title,
            shuffled,
        }
    }
}

mirror_struct! {
    crate::types::BridgeQueueSnapshot = bae_playback::ResolvedQueueSnapshot,
    from_core: pub(crate) fn,
    fields: {
        manual: (each crate::types::BridgeQueueEntry),
        context: (opt crate::types::BridgePlaybackContext),
        has_next,
        has_previous,
        manual_version,
    },
}
