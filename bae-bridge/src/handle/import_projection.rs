mirror_struct! {
    crate::types::BridgeFolderReleaseDecisionKey = bae_core::import::FolderReleaseDecisionKey,
    from_core: pub(super) fn,
    into_core: pub(super) fn,
    fields: { watched_folder_path, relative_folder_path },
}

mirror_enum! {
    crate::types::BridgeFolderScanStatus = bae_core::import::FolderScanStatus,
    from_core: fn,
    variants: { Scanning { found_count }, Complete, Failed { error } },
}

mirror_struct! {
    crate::types::BridgeWatchedFolderScanStatus = bae_core::import::WatchedFolderScanStatus,
    from_core: pub(super) fn,
    fields: {
        watched_folder_path,
        watched_folder_name,
        on_network_volume,
        status: (crate::types::BridgeFolderScanStatus),
    },
}

impl crate::types::BridgeFolderCandidate {
    pub(super) fn from_core(
        candidate: bae_core::import::FolderCandidate,
        skipped: bool,
        is_added: bool,
    ) -> Self {
        let track_count = candidate.files.track_count();
        crate::types::BridgeFolderCandidate {
            parts: candidate
                .files
                .parts
                .iter()
                .map(crate::types::BridgeReleasePart::from_core)
                .collect(),
            folder_path: candidate.key(),
            source_folder_name: candidate.name.clone(),
            watched_folder_path: candidate.watched_folder_path.clone(),
            files: crate::types::BridgeCandidateFiles::from_core(candidate.files),
            track_count,
            skipped,
            is_added,
        }
    }
}

impl crate::types::BridgeInvalidCandidate {
    pub(super) fn from_core(candidate: bae_core::import::InvalidCandidate) -> Self {
        let candidate_key = candidate.key();
        let bae_core::import::InvalidCandidate {
            path,
            name,
            watched_folder_path,
            display_path,
            grouping,
            reason,
        } = candidate;
        crate::types::BridgeInvalidCandidate {
            candidate_key,
            folder_path: path.to_string_lossy().to_string(),
            source_folder_name: name,
            watched_folder_path,
            display_path,
            separable: grouping.is_some(),
            reason: crate::types::BridgeInvalidReason::from_core(reason),
        }
    }
}

impl crate::types::BridgeCandidateRuntimeSnapshot {
    pub(crate) fn from_core(runtime: bae_core::import::CandidateRuntimeSnapshot) -> Self {
        // A failed write reaches the UI through the row's live state.
        let bae_core::import::CandidateRuntimeSnapshot {
            queued,
            running,
            saving,
            save_failed: _,
            import,
            search,
        } = runtime;
        crate::types::BridgeCandidateRuntimeSnapshot {
            identification: bae_core::import::IdentificationInFlight::of(queued, running, saving)
                .map(crate::types::BridgeIdentificationInFlight::from_core),
            import: import.map(crate::types::BridgeImportInFlight::from_core),
            search: search.map(crate::types::BridgeCandidateSearch::from_core),
        }
    }
}

impl crate::types::BridgeIdentificationInFlight {
    fn from_core(identification: bae_core::import::IdentificationInFlight) -> Self {
        match identification {
            bae_core::import::IdentificationInFlight::Queued => Self::Queued,
            bae_core::import::IdentificationInFlight::Run(state) => Self::Run {
                state: crate::types::BridgeIdentifyState::from_core(state),
            },
        }
    }
}

mirror_struct! {
    crate::types::BridgeImportInFlight = bae_core::import::ImportInFlight,
    from_core: fn,
    fields: {
        progress_percent,
        step: (crate::types::BridgeImportStep),
    },
}

impl crate::types::BridgeCandidateRuntimeChange {
    pub(super) fn from_core(change: bae_core::import::CandidateRuntimeChange) -> Self {
        match change {
            bae_core::import::CandidateRuntimeChange::Updated { key, runtime } => Self::Updated {
                key,
                runtime: crate::types::BridgeCandidateRuntimeSnapshot::from_core(runtime),
            },
            bae_core::import::CandidateRuntimeChange::Removed { key } => Self::Removed { key },
            bae_core::import::CandidateRuntimeChange::Reset { runtimes } => Self::Reset {
                runtimes: runtimes
                    .into_iter()
                    .map(|(key, runtime)| crate::types::BridgeKeyedCandidateRuntime {
                        key,
                        runtime: crate::types::BridgeCandidateRuntimeSnapshot::from_core(runtime),
                    })
                    .collect(),
            },
        }
    }
}

impl crate::types::BridgeTriageImportStatus {
    pub(super) fn from_core(status: bae_core::import::triage::TriageImportStatus) -> Self {
        match status {
            bae_core::import::triage::TriageImportStatus::Complete { release } => Self::Complete {
                release_id: release.release_id,
                album_id: release.album_id,
            },
            bae_core::import::triage::TriageImportStatus::Error { failure } => Self::Error {
                error: crate::types::BridgeError::from_core(failure.ui_error()),
            },
            bae_core::import::triage::TriageImportStatus::Blocked { reason } => Self::Error {
                error: crate::types::BridgeError::from(&reason),
            },
        }
    }
}

impl crate::types::BridgeCandidateImportStatus {
    pub(super) fn from_core(status: bae_core::import::CandidateImportStatus) -> Self {
        match status {
            bae_core::import::CandidateImportStatus::Importing { standing } => Self::Importing {
                standing: crate::types::BridgeImportStanding::from_core(standing),
            },
            bae_core::import::CandidateImportStatus::Complete { release } => Self::Complete {
                release_id: release.release_id,
                album_id: release.album_id,
            },
            bae_core::import::CandidateImportStatus::Error { failure } => Self::Error {
                error: crate::types::BridgeError::from_core(failure.ui_error()),
            },
            bae_core::import::CandidateImportStatus::Blocked { reason } => Self::Error {
                error: crate::types::BridgeError::from(&reason),
            },
        }
    }
}

// ── Sidebar triage: mirrors of `bae_core::import::triage` ─────────────────

impl crate::types::BridgeTriageRow {
    /// Not a copy: the row crosses with `live`, what the list joined to it.
    pub(crate) fn from_core(
        row: bae_core::import::TriageRow,
        live: bae_core::import::CandidateLiveState,
    ) -> Self {
        let bae_core::import::TriageRow {
            candidate_key,
            folder_name,
            watched_folder_path,
            display_path,
            actionable,
            placement,
            // What the row's commands are decided from, which `live` already
            // decided.
            action_basis: _,
            matched,
            metadata_summary,
            cover,
            import_status,
            metadata_provenance,
            // The row's reading names the linked release's records.
            release_link: _,
            reading,
            selected,
        } = row;
        crate::types::BridgeTriageRow {
            candidate_key,
            folder_name,
            watched_folder_path,
            display_path,
            actionable,
            placement: crate::types::BridgeTriagePlacement::from_core(placement),
            live: crate::types::BridgeCandidateLiveState::from_core(live),
            matched: matched.map(crate::types::BridgeMatchedRelease::from_core),
            metadata_summary: metadata_summary
                .map(crate::types::BridgeTriageMetadataSummary::from_core),
            cover: cover.map(crate::types::BridgeCoverImageSource::from_core),
            import_status: import_status.map(crate::types::BridgeTriageImportStatus::from_core),
            metadata_provenance: metadata_provenance
                .map(crate::types::BridgeMetadataProvenance::from_core),
            reading: crate::types::BridgeTriageReading::from_core(reading),
            selected,
        }
    }
}

mirror_struct! {
    crate::types::BridgeImportedReleaseSummary = bae_core::import::ImportedReleaseSummary,
    from_core: fn,
    fields: {
        release_id,
        album_id,
        title,
        artist,
        year,
        cover: (opt crate::types::BridgeImageRef),
        records: (each crate::types::BridgeReleaseRecord),
    },
}

impl crate::types::BridgeImportedRow {
    /// Not a copy: the row crosses with `live`, what the list joined to it.
    fn from_core(
        row: bae_core::import::ImportedRow,
        live: bae_core::import::CandidateLiveState,
    ) -> Self {
        let bae_core::import::ImportedRow {
            candidate_key,
            display_path,
            // What the row's commands are decided from, which `live` already
            // decided.
            action_basis: _,
            release,
            selected,
        } = row;
        Self {
            candidate_key,
            display_path,
            live: crate::types::BridgeCandidateLiveState::from_core(live),
            release: crate::types::BridgeImportedReleaseSummary::from_core(release),
            selected,
        }
    }
}

mirror_enum! {
    crate::types::BridgeCandidateAction = bae_core::import::triage::CandidateAction,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        Import,
        Identify,
        CancelIdentification,
        CancelImport,
        RetryIdentification,
        ResetToFileMetadata,
        ClearMetadata,
        Combine,
        Separate,
        Skip,
        Restore,
        RevealFolder,
    },
}

mirror_enum! {
    crate::types::BridgeTriageTab = bae_core::import::TriageTab,
    from_core: pub(super) fn,
    into_core: pub(super) fn,
    variants: { Pending, Done, Skipped },
}

mirror_enum! {
    crate::types::BridgeTriagePlacement = bae_core::import::TriagePlacement,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Pending, Failed, Done, Skipped },
}

mirror_enum! {
    crate::types::BridgePendingStanding = bae_core::import::PendingStanding,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        NotLookedUp,
        Identifying,
        NeedsYou { reason: (crate::types::BridgeNeedsYouReason) },
        Identified,
        Unmatched,
        LookupError,
        Error { failure: (crate::types::BridgeInternalFailure) },
        Importing,
        ImportError,
    },
}

mirror_enum! {
    crate::types::BridgeNeedsYouReason = bae_core::import::NeedsYouReason,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        Matches { count },
        NoTracklist,
        MediumMismatch { folder: (crate::types::BridgeMediumMismatch), releases },
        NotFound,
        NothingToLookUp,
    },
}

mirror_enum! {
    crate::types::BridgeMediumMismatch = bae_core::identify::MediumConflict,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { CdRip, NotCdAudio },
}

impl crate::types::BridgeCandidateLiveState {
    pub(crate) fn from_core(live: bae_core::import::CandidateLiveState) -> Self {
        let bae_core::import::CandidateLiveState {
            facts,
            actions,
            standing,
        } = live;
        let bae_core::import::TriageRuntimeFacts {
            identification,
            import,
        } = facts;
        Self {
            identification: identification.map(crate::types::BridgeIdentificationStatus::from_core),
            import: import.map(crate::types::BridgeImportStanding::from_core),
            actions: actions
                .into_iter()
                .map(crate::types::BridgeCandidateAction::from_core)
                .collect(),
            badge: standing
                .as_ref()
                .and_then(bae_core::import::PendingStanding::badge)
                .map(crate::types::BridgeRowBadge::from_core),
            standing: standing.map(crate::types::BridgePendingStanding::from_core),
        }
    }
}

impl crate::types::BridgeRowBadge {
    fn from_core(badge: bae_core::import::PendingBadge) -> Self {
        Self {
            says: crate::types::BridgePendingBadge::from_core(badge),
            tone: crate::types::BridgeBadgeTone::from_core(badge.tone()),
        }
    }
}

mirror_enum! {
    crate::types::BridgePendingBadge = bae_core::import::PendingBadge,
    from_core: fn,
    variants: {
        NeedsYou { reason: (crate::types::BridgeNeedsYouReason) },
        LookupError,
        Error,
        ImportError,
    },
}

mirror_enum! {
    crate::types::BridgeBadgeTone = bae_core::import::BadgeTone,
    from_core: fn,
    variants: { Attention, Failure },
}

mirror_enum! {
    crate::types::BridgeImportStanding = bae_core::import::ImportStanding,
    from_core: fn,
    variants: { Queued, Running, Writing },
}

impl crate::types::BridgeIdentificationStatus {
    pub(crate) fn from_core(status: bae_core::import::IdentificationStatus) -> Self {
        use bae_core::import::IdentificationStatus as S;
        match status {
            S::Queued => Self::Queued,
            S::Running => Self::Running,
            S::Finalizing => Self::Finalizing,
            S::FinalizationFailed { failure } => Self::FinalizationFailed {
                error: crate::types::BridgeError::from_core(bae_core::ui::UiError::import(
                    failure.error().to_string(),
                )),
            },
        }
    }
}

// Not a copy: audio no CD holds crosses with the rate its files state, which
// only the pane has at hand.
impl crate::types::BridgeFolderCheck {
    /// A check with the rate of the audio's files.
    pub(crate) fn with_rate(check: bae_core::identify::FolderCheck, rate: Option<u32>) -> Self {
        use bae_core::identify::FolderCheck;
        match check {
            FolderCheck::SourceTracksUnknown => Self::SourceTracksUnknown,
            FolderCheck::MediumDisagrees { folder } => Self::MediumDisagrees {
                folder: match folder {
                    bae_core::identify::MediumConflict::CdRip => {
                        crate::types::BridgeMediumConflict::CdRip
                    }
                    bae_core::identify::MediumConflict::NotCdAudio => {
                        crate::types::BridgeMediumConflict::NotCdAudio {
                            sample_rate_hz: rate,
                        }
                    }
                },
            },
        }
    }
}

mirror_struct! {
    crate::types::BridgeMatchedPressing = bae_core::import::MatchedPressing,
    from_core: fn,
    fields: { year, media: (each crate::types::BridgeMediaCount), track_count },
}

mirror_struct! {
    crate::types::BridgeMatchEvidence = bae_core::import::MatchEvidence,
    from_core: fn,
    fields: {
        source: (crate::types::BridgeCatalog),
        signal: (opt crate::types::BridgeMatchedSignal),
    },
}

mirror_struct! {
    crate::types::BridgeMatchedRelease = bae_core::import::MatchedRelease,
    from_core: pub(crate) fn,
    fields: {
        release_id,
        title,
        artist,
        pressing: (opt crate::types::BridgeMatchedPressing),
        cover: (opt crate::types::BridgeRemoteImageSet),
        evidence: (crate::types::BridgeMatchEvidence),
    },
}

mirror_enum! {
    crate::types::BridgeMatchedSignal = bae_core::import::MatchedSignal,
    from_core: pub(crate) fn,
    variants: { DiscId, Barcode, Isrc, TitleSearch },
}

// ── The paged list ─────────────────────────────────────────────────────────

mirror_struct! {
    crate::types::BridgeTriageMetadataSummary = bae_core::import::triage::TriageMetadataSummary,
    from_core: fn,
    fields: {
        album_title,
        album_artist_assignments: (each crate::types::BridgeArtistAssignment),
    },
}

mirror_enum! {
    crate::types::BridgeImportListOrder = bae_core::import::ImportListOrder,
    into_core: fn,
    variants: { NewestFirst, OldestFirst, PathAscending, PathDescending },
}

mirror_enum! {
    crate::types::BridgePendingFilter = bae_core::import::PendingFilter,
    from_core: fn,
    into_core: fn,
    variants: { All, NeedsYou, InProgress, Identified, Unmatched, NotLookedUp },
}

mirror_struct! {
    crate::types::BridgePendingFilterEntry = bae_core::import::PendingFilterEntry,
    from_core: pub(super) fn,
    fields: {
        filter: (crate::types::BridgePendingFilter),
        count,
        selectable,
    },
}

mirror_struct! {
    crate::types::BridgeImportListNarrowing = bae_core::import::ImportListNarrowing,
    from_core: fn,
    fields: {
        tab: (crate::types::BridgeTriageTab),
        filter_text,
        pending_filter: (crate::types::BridgePendingFilter),
    },
}

mirror_struct! {
    crate::types::BridgeImportListView = bae_core::import::ImportListView,
    into_core: pub(super) fn,
    fields: {
        tab: (crate::types::BridgeTriageTab),
        filter_text,
        pending_filter: (crate::types::BridgePendingFilter),
        collapsed_groups: (each crate::types::BridgeFolderReleaseDecisionKey),
        order: (crate::types::BridgeImportListOrder),
    },
}

impl crate::types::BridgeImportListItem {
    pub(super) fn from_core(
        item: bae_core::import::ImportListItem<bae_core::import::CandidateLiveState>,
    ) -> Self {
        let stable_key = item.stable_key();
        match item {
            bae_core::import::ImportListItem::GroupHeader {
                group,
                watched_folder_path,
                expanded,
                entry_count,
            } => Self::GroupHeader {
                stable_key,
                group: crate::types::BridgeTriageGroup {
                    key: crate::types::BridgeFolderReleaseDecisionKey::from_core(group.key),
                    name: group.name,
                    combinable: group.combinable,
                },
                watched_folder_path,
                expanded,
                entry_count,
            },
            bae_core::import::ImportListItem::Candidate {
                row,
                live,
                is_group_member,
            } => Self::Candidate {
                stable_key,
                row: crate::types::BridgeTriageRow::from_core(row, live),
                is_group_member,
            },
            bae_core::import::ImportListItem::Imported { row, live } => Self::Imported {
                stable_key,
                row: crate::types::BridgeImportedRow::from_core(row, live),
            },
            bae_core::import::ImportListItem::Invalid {
                candidate,
                is_group_member,
            } => Self::Invalid {
                stable_key,
                invalid_candidate: crate::types::BridgeInvalidCandidate::from_core(candidate),
                is_group_member,
            },
        }
    }
}

mirror_struct! {
    crate::types::BridgeTriageTabCounts = bae_core::import::TriageTabCounts,
    from_core: fn,
    fields: { pending, done, skipped },
}

mirror_struct! {
    crate::types::BridgeActiveFolderScan = bae_core::import::ActiveFolderScan,
    from_core: fn,
    fields: { watched_folder_path, watched_folder_name, found_count },
}

mirror_struct! {
    crate::types::BridgeFolderScanActivity = bae_core::import::FolderScanActivity,
    from_core: fn,
    fields: {
        found_count,
        folders: (each crate::types::BridgeActiveFolderScan),
    },
}

mirror_struct! {
    crate::types::BridgeNarrowedCount = bae_core::import::NarrowedCount,
    from_core: fn,
    fields: { shown, total },
}

impl crate::types::BridgeImportQueueSummary {
    /// The list's summary and where the folder scans stand.
    fn from_core(
        summary: bae_core::import::ImportQueueSummary,
        folder_scans: bae_core::import::FolderScanProgress,
    ) -> Self {
        let bae_core::import::ImportQueueSummary {
            counts,
            watched_folders,
            group_keys,
            pending_covers,
            narrowed,
            narrowing,
            first_selected_position,
        } = summary;
        let bae_core::import::FolderScanProgress { statuses, activity } = folder_scans;
        Self {
            counts: crate::types::BridgeTriageTabCounts::from_core(counts),
            watched_folders: watched_folders
                .into_iter()
                .map(crate::types::BridgeWatchedFolder::from_core)
                .collect(),
            folder_scan_statuses: statuses
                .into_iter()
                .map(crate::types::BridgeWatchedFolderScanStatus::from_core)
                .collect(),
            folder_scan_activity: activity.map(crate::types::BridgeFolderScanActivity::from_core),
            group_keys: group_keys
                .into_iter()
                .map(crate::types::BridgeFolderReleaseDecisionKey::from_core)
                .collect(),
            pending_covers: pending_covers
                .into_iter()
                .map(crate::types::BridgeRemoteImageSet::from_core)
                .collect(),
            narrowed: narrowed.map(crate::types::BridgeNarrowedCount::from_core),
            narrowing: crate::types::BridgeImportListNarrowing::from_core(narrowing),
            first_selected_position,
        }
    }
}

mirror_struct! {
    crate::types::BridgeImportCandidateListLocation
        = bae_core::import::ImportCandidateListLocation,
    from_core: pub(super) fn,
    fields: {
        stable_key,
        tab: (crate::types::BridgeTriageTab),
        group_key: (opt crate::types::BridgeFolderReleaseDecisionKey),
        visible_position,
    },
}

mirror_struct! {
    crate::types::BridgeImportListWindow
        = bae_core::import::ImportListWindow<bae_core::import::CandidateLiveState>,
    from_core: fn,
    fields: {
        window: (crate::types::BridgeLibraryPageWindow),
        items: (each crate::types::BridgeImportListItem),
    },
}

impl crate::types::BridgeImportListSnapshot {
    pub(super) fn from_core(snapshot: bae_core::import::ImportListSnapshot) -> Self {
        let bae_core::import::ImportListSnapshot {
            windows,
            total_count,
            summary,
            folder_scans,
            selection_revision,
            request_revision,
            cause,
        } = snapshot;
        Self {
            windows: windows
                .into_iter()
                .map(crate::types::BridgeImportListWindow::from_core)
                .collect(),
            total_count,
            summary: crate::types::BridgeImportQueueSummary::from_core(summary, folder_scans),
            selection_revision,
            request_revision,
            cause: crate::types::BridgeLiveQueryCause::from_core(cause),
        }
    }
}

impl crate::types::BridgeCandidatePanePlacement {
    /// Not a copy: the folder check crosses with `rate`, the rate of the
    /// audio's files.
    fn from_core(placement: bae_core::import::CandidatePanePlacement, rate: Option<u32>) -> Self {
        let records = |records: Vec<bae_core::import::ReleaseRecord>| {
            records
                .into_iter()
                .map(crate::types::BridgeReleaseRecord::from_core)
                .collect()
        };
        match placement {
            bae_core::import::CandidatePanePlacement::Pending {
                folder_check,
                records: pending,
            } => Self::Pending {
                folder_check: folder_check
                    .map(|check| crate::types::BridgeFolderCheck::with_rate(check, rate)),
                records: records(pending),
            },
            bae_core::import::CandidatePanePlacement::Skipped { records: skipped } => {
                Self::Skipped {
                    records: records(skipped),
                }
            }
            bae_core::import::CandidatePanePlacement::Done => Self::Done,
        }
    }
}

impl crate::types::BridgeImportCandidateDetail {
    pub(super) fn from_core(detail: bae_core::import::ImportCandidateDetail) -> Self {
        let bae_core::import::ImportCandidateDetail {
            candidate,
            actionable,
            skipped,
            is_added,
            resumed_identify_state,
            placement,
            live,
            import_status,
            release,
            picked_library_status,
            file_evidence,
            metadata_draft,
            artist_resolutions,
            metadata_draft_is_blank,
            metadata_provenance,
            release_link,
            metadata_author,
            metadata_revision,
            mapping,
            cover,
            // The same list as `release.cover_art`.
            remote_covers: _,
            signals,
            // Changed through `edit_candidate_lookup_choices`; nothing on the
            // pane reads it back.
            lookup_choices: _,
            failure,
            session,
        } = detail;
        let rate = resumed_identify_state
            .audio()
            .and_then(|audio| audio.rate_ruling_out_cd);
        Self {
            candidate: crate::types::BridgeFolderCandidate::from_core(candidate, skipped, is_added),
            actionable,
            resumed_identify_state: crate::types::BridgeIdentifyState::from_core(
                resumed_identify_state,
            ),
            placement: crate::types::BridgeCandidatePanePlacement::from_core(placement, rate),
            live: crate::types::BridgeCandidateLiveState::from_core(live),
            import_status: import_status.map(crate::types::BridgeCandidateImportStatus::from_core),
            release: release.map(crate::types::BridgeReleaseDetail::from_core),
            picked_library_status: picked_library_status
                .map(crate::types::BridgeLibraryStatus::from_core),
            file_evidence: file_evidence
                .into_iter()
                .map(crate::types::BridgeFileEvidence::from_core)
                .collect(),
            metadata_draft: crate::types::BridgeRawReleaseEdit::from_core(metadata_draft),
            artist_resolutions: artist_resolutions
                .into_iter()
                .map(crate::types::BridgeResolvedCredit::from_core)
                .collect(),
            metadata_draft_is_blank,
            metadata_provenance: metadata_provenance
                .map(crate::types::BridgeMetadataProvenance::from_core),
            release_link: release_link.map(crate::types::BridgeReleaseLink::from_core),
            metadata_author: crate::types::BridgeMetadataAuthor::from_core(metadata_author),
            metadata_revision,
            mapping: crate::types::BridgeMappingTable::from_core(mapping),
            cover: cover.map(crate::types::BridgeCoverChoice::from_core),
            signals: signals.map(crate::types::BridgeSignals::from_core),
            failure: failure.map(crate::types::BridgeImportFailure::from_core),
            session: crate::types::BridgeCandidateSession::from_core(session),
        }
    }
}

impl crate::types::BridgeImportFailure {
    fn from_core(failure: bae_core::import::ImportFailure) -> Self {
        let bae_core::import::ImportFailure {
            reason,
            failed_at: _,
            artist_identity_conflict,
        } = failure;
        let error = crate::types::BridgeError::from_core(reason.ui_error());
        match (reason, artist_identity_conflict) {
            (
                bae_core::import::ImportFailureReason::AlreadyInLibrary {
                    album_id,
                    album_title,
                },
                _,
            ) => Self::AlreadyInLibrary {
                album_id,
                album_title,
            },
            (bae_core::import::ImportFailureReason::Error { .. }, Some(conflict)) => {
                Self::ArtistIdentityConflict {
                    conflict: crate::types::BridgeArtistIdentityConflict {
                        incoming_artist_name: conflict.incoming_artist_name,
                        discogs_artist: crate::types::BridgeExistingArtist::from_core(
                            conflict.discogs_artist,
                        ),
                        musicbrainz_artist: crate::types::BridgeExistingArtist::from_core(
                            conflict.musicbrainz_artist,
                        ),
                    },
                    error,
                }
            }
            (bae_core::import::ImportFailureReason::Error { .. }, None) => Self::Other { error },
        }
    }
}

mirror_enum! {
    crate::types::BridgeSelectionChange = bae_core::import::selection::SelectionChange,
    into_core: pub(super) fn,
    variants: {
        Replace { keys },
        Toggle { add, remove },
        Extend { from, to },
    },
}
