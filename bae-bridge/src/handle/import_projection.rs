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
        grouping_action: Option<bae_core::import::grouping::GroupingAction>,
    ) -> Self {
        let track_count = candidate.files.track_count();
        crate::types::BridgeFolderCandidate {
            grouping_action: grouping_action.map(crate::types::BridgeGroupingAction::from_core),
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
        // The queue marker and a failed write are the row's facts, drawn from
        // the candidate's triage status; the pane draws the run itself, and
        // reads the one in flight before the one being written.
        let bae_core::import::CandidateRuntimeSnapshot {
            queued: _,
            running,
            saving,
            save_failed: _,
            import,
            search,
        } = runtime;
        let identify = running.or(saving);
        crate::types::BridgeCandidateRuntimeSnapshot {
            identify_state: crate::types::BridgeIdentifyState::from_core(
                identify.unwrap_or(bae_core::identify::IdentifyState::Idle),
            ),
            import: import.map(crate::types::BridgeImportInFlight::from_core),
            search: search.map(crate::types::BridgeCandidateSearch::from_core),
        }
    }
}

mirror_struct! {
    crate::types::BridgeImportInFlight = bae_core::import::ImportInFlight,
    from_core: fn,
    fields: {
        progress_percent,
        step: (opt crate::types::BridgeImportStep),
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
            bae_core::import::CandidateRuntimeChange::Reset { runtimes } => Self::reset(runtimes),
        }
    }

    /// Every key in flight right now, as the one change a consumer that
    /// dropped deliveries can rebuild itself from.
    pub(crate) fn reset(
        runtimes: std::collections::HashMap<String, bae_core::import::CandidateRuntimeSnapshot>,
    ) -> Self {
        Self::Reset {
            runtimes: runtimes
                .into_iter()
                .map(|(key, runtime)| crate::types::BridgeKeyedCandidateRuntime {
                    key,
                    runtime: crate::types::BridgeCandidateRuntimeSnapshot::from_core(runtime),
                })
                .collect(),
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
            bae_core::import::triage::TriageImportStatus::Error { error } => Self::Error {
                error: crate::types::BridgeError::from_core(bae_core::ui::UiError::import(error)),
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
            bae_core::import::CandidateImportStatus::Importing => Self::Importing,
            bae_core::import::CandidateImportStatus::Complete { release } => Self::Complete {
                release_id: release.release_id,
                album_id: release.album_id,
            },
            bae_core::import::CandidateImportStatus::Error { error } => Self::Error {
                error: crate::types::BridgeError::from_core(bae_core::ui::UiError::import(error)),
            },
            bae_core::import::CandidateImportStatus::Blocked { reason } => Self::Error {
                error: crate::types::BridgeError::from(&reason),
            },
        }
    }
}

// ── Sidebar triage ─────────────────────────────────────────────────────────
//
// A mirror, variant for variant. Every decision behind these values was made in
// `bae_core::import::triage`.

impl crate::types::BridgeTriageRow {
    pub(crate) fn from_core(row: bae_core::import::TriageRow) -> Self {
        let bae_core::import::TriageRow {
            candidate_key,
            folder_name,
            watched_folder_path,
            display_path,
            separable,
            actionable,
            placement,
            ready_check,
            action_basis,
            matched,
            metadata_summary,
            cover,
            selectable,
            import_status,
            metadata_provenance,
            reading,
        } = row;
        crate::types::BridgeTriageRow {
            candidate_key,
            folder_name,
            watched_folder_path,
            display_path,
            separable,
            actionable,
            placement: crate::types::BridgeTriagePlacement::from_core(placement),
            ready_check: ready_check.map(crate::types::BridgeNeedsYou::from_core),
            action_basis: crate::types::BridgeCandidateActionBasis::from_core(action_basis),
            matched: matched.map(crate::types::BridgeMatchedRelease::from_core),
            metadata_summary: metadata_summary
                .map(crate::types::BridgeTriageMetadataSummary::from_core),
            cover: cover.map(crate::types::BridgeCoverImageSource::from_core),
            selectable,
            import_status: import_status.map(crate::types::BridgeTriageImportStatus::from_core),
            metadata_provenance: metadata_provenance
                .map(crate::types::BridgeMetadataProvenance::from_core),
            reading: crate::types::BridgeTriageReading::from_core(reading),
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

mirror_struct! {
    crate::types::BridgeImportedRow = bae_core::import::ImportedRow,
    from_core: fn,
    fields: {
        candidate_key,
        display_path,
        action_basis: (crate::types::BridgeCandidateActionBasis),
        release: (crate::types::BridgeImportedReleaseSummary),
    },
}

mirror_enum! {
    crate::types::BridgeCandidateAction = bae_core::import::triage::CandidateAction,
    from_core: fn,
    variants: {
        ImportReady,
        Identify,
        RetryIdentification,
        ResetToFileMetadata,
        ClearMetadata,
        Skip,
        Restore,
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
    variants: {
        Pending,
        Ready,
        NeedsYou { reason: (crate::types::BridgeNeedsYou) },
        Failed,
        Done,
        Skipped,
    },
}

mirror_struct! {
    crate::types::BridgeCandidateActionBasis = bae_core::import::CandidateActionBasis,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: {
        actionable,
        placement: (crate::types::BridgeTriagePlacement),
        lookup_failed,
    },
}

impl crate::types::BridgeCandidateLiveState {
    pub(crate) fn from_core(live: bae_core::import::CandidateLiveState) -> Self {
        let bae_core::import::CandidateLiveState { facts, actions } = live;
        let bae_core::import::TriageRuntimeFacts {
            identification,
            importing,
        } = facts;
        Self {
            identification: identification.map(crate::types::BridgeIdentificationStatus::from_core),
            importing,
            actions: actions
                .into_iter()
                .map(crate::types::BridgeCandidateAction::from_core)
                .collect(),
        }
    }
}

impl crate::types::BridgeIdentificationStatus {
    pub(crate) fn from_core(status: bae_core::import::IdentificationStatus) -> Self {
        use bae_core::import::IdentificationStatus as S;
        match status {
            S::Queued => Self::Queued,
            S::Running => Self::Running,
            S::Finalizing => Self::Finalizing,
            S::FinalizationFailed { error } => Self::FinalizationFailed {
                error: crate::types::BridgeError::from_core(bae_core::ui::UiError::import(error)),
            },
        }
    }
}

mirror_enum! {
    crate::types::BridgeNeedsYou = bae_core::identify::NeedsYou,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        SeveralMatches { count },
        NoMatch,
        NothingToLookUp,
        LookupFailed,
        TrackCountDisagrees { local, source },
        SourceTracksUnknown,
    },
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
    variants: { DiscId, Barcode, TitleSearch },
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

mirror_struct! {
    crate::types::BridgeImportListView = bae_core::import::ImportListView,
    into_core: pub(super) fn,
    fields: {
        tab: (crate::types::BridgeTriageTab),
        filter_text,
        collapsed_groups: (each crate::types::BridgeFolderReleaseDecisionKey),
        order: (crate::types::BridgeImportListOrder),
    },
}

impl crate::types::BridgeImportListItem {
    pub(super) fn from_core(item: bae_core::import::ImportListItem) -> Self {
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
                is_group_member,
            } => Self::Candidate {
                stable_key,
                row: crate::types::BridgeTriageRow::from_core(row),
                is_group_member,
            },
            bae_core::import::ImportListItem::Imported { row } => Self::Imported {
                stable_key,
                row: crate::types::BridgeImportedRow::from_core(row),
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
    crate::types::BridgeReadyRowRef = bae_core::import::ReadyRowRef,
    from_core: fn,
    fields: { candidate_key, cover: (opt crate::types::BridgeRemoteImageSet) },
}

impl crate::types::BridgeImportQueueSummary {
    /// The list's chrome and where the folder scans stand, which core reads
    /// by two live queries and delivers side by side.
    fn from_core(
        summary: bae_core::import::ImportQueueSummary,
        folder_scans: bae_core::import::FolderScanProgress,
    ) -> Self {
        let bae_core::import::ImportQueueSummary {
            counts,
            watched_folders,
            group_keys,
            ready,
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
            ready: ready
                .into_iter()
                .map(crate::types::BridgeReadyRowRef::from_core)
                .collect(),
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
    crate::types::BridgeImportListWindow = bae_core::import::ImportListWindow,
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
            request_revision,
            cause: crate::types::BridgeLiveQueryCause::from_core(cause),
        }
    }
}

mirror_enum! {
    crate::types::BridgeCandidatePanePlacement = bae_core::import::CandidatePanePlacement,
    from_core: fn,
    variants: {
        Pending {
            ready_check: (opt crate::types::BridgeNeedsYou),
            records: (each crate::types::BridgeReleaseRecord),
        },
        Skipped { records: (each crate::types::BridgeReleaseRecord) },
        Done,
    },
}

impl crate::types::BridgeImportCandidateDetail {
    pub(super) fn from_core(detail: bae_core::import::ImportCandidateDetail) -> Self {
        let bae_core::import::ImportCandidateDetail {
            grouping_action,
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
            metadata_author,
            metadata_revision,
            mapping,
            cover,
            // Every cover the picker offers is already inside `release`, whose
            // `cover_art` is the same list.
            remote_covers: _,
            signals,
            lookup_choices,
            failure,
            session,
        } = detail;
        Self {
            candidate: crate::types::BridgeFolderCandidate::from_core(
                candidate,
                skipped,
                is_added,
                grouping_action,
            ),
            actionable,
            resumed_identify_state: crate::types::BridgeIdentifyState::from_core(
                resumed_identify_state,
            ),
            placement: crate::types::BridgeCandidatePanePlacement::from_core(placement),
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
            metadata_author: crate::types::BridgeMetadataAuthor::from_core(metadata_author),
            metadata_revision,
            mapping: crate::types::BridgeMappingTable::from_core(mapping),
            cover: cover.map(crate::types::BridgeCoverChoice::from_core),
            signals: signals.map(crate::types::BridgeSignals::from_core),
            lookup_choices: crate::types::BridgeLookupChoices::from_core(lookup_choices),
            failure: failure.map(crate::types::BridgeImportFailure::from_core),
            session: crate::types::BridgeCandidateSession::from_core(session),
        }
    }
}

impl crate::types::BridgeImportFailure {
    fn from_core(failure: bae_core::import::ImportFailure) -> Self {
        let artist_identity_conflict = failure.artist_identity_conflict.map(|conflict| {
            crate::types::BridgeArtistIdentityConflict {
                incoming_artist_name: conflict.incoming_artist_name,
                discogs_artist: crate::types::BridgeExistingArtist::from_core(
                    conflict.discogs_artist,
                ),
                musicbrainz_artist: crate::types::BridgeExistingArtist::from_core(
                    conflict.musicbrainz_artist,
                ),
            }
        });
        Self {
            error: crate::types::BridgeError::import(failure.error),
            artist_identity_conflict,
        }
    }
}
