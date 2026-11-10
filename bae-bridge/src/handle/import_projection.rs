impl crate::types::BridgeFolderReleaseDecisionKey {
    pub(super) fn from_core(key: bae_import::FolderReleaseDecisionKey) -> Self {
        let bae_import::FolderReleaseDecisionKey { watched_folder_path, relative_folder } = key;
        Self {
            watched_folder_path: coven::path_bytes(&watched_folder_path),
            relative_folder: relative_folder.as_bytes().to_vec(),
        }
    }

    /// The folder the app names. Only bytes no folder here has, which core
    /// never gave the app, fail.
    pub(crate) fn into_core(
        self,
    ) -> Result<bae_import::FolderReleaseDecisionKey, crate::types::BridgeError> {
        let Self { watched_folder_path, relative_folder } = self;
        Ok(bae_import::FolderReleaseDecisionKey {
            watched_folder_path: crate::from_app::path(watched_folder_path)?,
            relative_folder: bae_import::RelativeFolder::from_bytes(relative_folder).map_err(
                |error| {
                    crate::types::BridgeError::internal(format!("a folder from the app: {error}"))
                },
            )?,
        })
    }
}

mirror_enum! {
    crate::types::BridgeFolderScanStatus = bae_import::FolderScanStatus,
    from_core: fn,
    variants: { Scanning, Complete, Failed { error } },
}

impl crate::types::BridgeWatchedFolderScanStatus {
    pub(super) fn from_core(status: bae_import::WatchedFolderScanStatus) -> Self {
        let bae_import::WatchedFolderScanStatus {
            watched_folder_path,
            watched_folder_name,
            on_network_volume,
            status,
        } = status;
        Self {
            watched_folder_path: coven::path_bytes(&watched_folder_path),
            watched_folder_name,
            on_network_volume,
            status: crate::types::BridgeFolderScanStatus::from_core(status),
        }
    }
}

impl crate::types::BridgeFolderCandidate {
    pub(super) fn from_core(
        candidate: bae_import::FolderCandidate,
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
            candidate_key: Vec::<u8>::from(candidate.key()),
            folder_path: coven::path_bytes(&candidate.path),
            source_folder_name: candidate.name.clone(),
            watched_folder_path: coven::path_bytes(&candidate.watched_folder_path),
            files: crate::types::BridgeCandidateFiles::from_core(candidate.files),
            track_count,
            skipped,
            is_added,
        }
    }
}

impl crate::types::BridgeInvalidCandidate {
    pub(crate) fn from_core(candidate: bae_import::InvalidCandidate) -> Self {
        let candidate_key = candidate.key();
        let reason = crate::types::BridgeInvalidReason::from_core(candidate.reason());
        let cover = candidate.cover_image().map(crate::types::BridgeCoverImageSource::from_core);
        let bae_import::InvalidCandidate {
            path,
            name,
            watched_folder_path,
            relative_folder,
            grouping,
            files: _,
            tags,
        } = candidate;
        crate::types::BridgeInvalidCandidate {
            candidate_key: candidate_key.into(),
            folder_path: coven::path_bytes(&path),
            source_folder_name: name,
            watched_folder_path: coven::path_bytes(&watched_folder_path),
            display_path: relative_folder.text(),
            separable: grouping.is_some(),
            reason,
            tags: tags.map(crate::types::BridgeSkippedTags::from_core),
            cover,
        }
    }
}

impl crate::types::BridgeInvalidCandidateDetail {
    pub(crate) fn from_core(detail: bae_import::InvalidCandidateDetail) -> Self {
        let bae_import::InvalidCandidateDetail { candidate, images, files } = detail;
        Self {
            candidate: crate::types::BridgeInvalidCandidate::from_core(candidate),
            images: images.into_iter().map(crate::types::BridgeMappingImage::from_core).collect(),
            files: files.into_iter().map(crate::types::BridgeMappingFile::from_core).collect(),
        }
    }
}

impl crate::types::BridgeCandidateRuntimeSnapshot {
    pub(crate) fn from_core(runtime: bae_import::CandidateRuntimeSnapshot) -> Self {
        // A failed write, and whether an Import was asked of the answer,
        // reach the UI through the row's live state.
        let in_flight = runtime.identification_in_flight();
        let signals = match &in_flight {
            Some(bae_import::IdentificationInFlight::Run(published)) => {
                published.signals.clone().map(crate::types::BridgeSignals::from_core)
            }
            Some(bae_import::IdentificationInFlight::Queued) | None => None,
        };
        let bae_import::CandidateRuntimeSnapshot { identification: _, import, search } = runtime;
        crate::types::BridgeCandidateRuntimeSnapshot {
            identification: in_flight.map(crate::types::BridgeIdentificationInFlight::from_core),
            import: import.map(crate::types::BridgeImportInFlight::from_core),
            search: search.map(crate::types::BridgeCandidateSearch::from_core),
            signals,
        }
    }
}

impl crate::types::BridgeIdentificationInFlight {
    fn from_core(identification: bae_import::IdentificationInFlight) -> Self {
        match identification {
            bae_import::IdentificationInFlight::Queued => Self::Queued,
            bae_import::IdentificationInFlight::Run(state) => {
                Self::Run { state: crate::types::BridgeIdentifyState::from_published(state) }
            }
        }
    }
}

mirror_struct! {
    crate::types::BridgeImportInFlight = bae_import::ImportInFlight,
    from_core: fn,
    fields: {
        step: (crate::types::BridgeImportStep),
    },
}

impl crate::types::BridgeTriageImportStatus {
    pub(super) fn from_core(status: bae_import::triage::TriageImportStatus) -> Self {
        match status {
            bae_import::triage::TriageImportStatus::Complete { release } => {
                Self::Complete { release: crate::types::BridgeImportedRelease::from_core(release) }
            }
            bae_import::triage::TriageImportStatus::Error { failure } => {
                Self::Error { error: crate::types::BridgeError::from_core(failure.ui_error()) }
            }
            bae_import::triage::TriageImportStatus::Blocked { reason } => {
                Self::Error { error: crate::types::BridgeError::from(&reason) }
            }
        }
    }
}

impl crate::types::BridgeCandidateImportStatus {
    pub(super) fn from_core(status: bae_import::CandidateImportStatus) -> Self {
        match status {
            bae_import::CandidateImportStatus::Importing { step } => {
                Self::Importing { step: crate::types::BridgeImportStep::from_core(step) }
            }
            bae_import::CandidateImportStatus::Complete { release } => {
                Self::Complete { release: crate::types::BridgeImportedRelease::from_core(release) }
            }
            bae_import::CandidateImportStatus::Error { failure } => {
                Self::Error { error: crate::types::BridgeError::from_core(failure.ui_error()) }
            }
            bae_import::CandidateImportStatus::Blocked { reason } => {
                Self::Error { error: crate::types::BridgeError::from(&reason) }
            }
        }
    }
}

// ── Sidebar triage: mirrors of `bae_import::triage` ─────────────────

impl crate::types::BridgeTriageRow {
    /// Not a copy: the row crosses with `live`, what the list joined to it.
    pub(crate) fn from_core(
        row: bae_import::TriageRow,
        live: bae_import::CandidateLiveState,
    ) -> Self {
        let bae_import::TriageRow {
            candidate_key,
            folder_name,
            watched_folder_path,
            display_path,
            actionable,
            placement,
            // What the row's commands are decided from, which `live` already
            // decided.
            action_basis: _,
            metadata_summary,
            cover,
            import_status,
            upload,
            metadata_provenance,
            // The row's reading names the linked release's records.
            release_link: _,
            reading,
            selected,
        } = row;
        crate::types::BridgeTriageRow {
            candidate_key: candidate_key.into(),
            folder_name,
            watched_folder_path: coven::path_bytes(&watched_folder_path),
            display_path,
            actionable,
            placement: crate::types::BridgeTriagePlacement::from_core(placement),
            live: crate::types::BridgeCandidateLiveState::from_core(live),
            metadata_summary: metadata_summary
                .map(crate::types::BridgeTriageMetadataSummary::from_core),
            cover: cover.map(crate::types::BridgeCoverImageSource::from_core),
            import_status: import_status.map(crate::types::BridgeTriageImportStatus::from_core),
            upload: upload.map(crate::types::BridgeUploadProgress::from_core),
            metadata_provenance: metadata_provenance
                .map(crate::types::BridgeMetadataProvenance::from_core),
            reading: crate::types::BridgeTriageReading::from_core(reading),
            selected,
        }
    }
}

mirror_struct! {
    crate::types::BridgeImportedRelease = bae_import::ImportedRelease,
    from_core: fn,
    fields: { release_id, album_id },
}

mirror_enum! {
    crate::types::BridgeCandidateAction = bae_import::triage::CandidateAction,
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
    crate::types::BridgeTriageTab = bae_import::TriageTab,
    from_core: pub(super) fn,
    into_core: pub(super) fn,
    variants: { Pending, Done, Skipped },
}

mirror_enum! {
    crate::types::BridgeTriagePlacement = bae_import::TriagePlacement,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Pending, Failed, Done, Skipped },
}

mirror_enum! {
    crate::types::BridgePendingStanding = bae_import::PendingStanding,
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
    crate::types::BridgeNeedsYouReason = bae_import::NeedsYouReason,
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
    crate::types::BridgeMediumMismatch = bae_identify::MediumConflict,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { CdRip, NotCdAudio },
}

impl crate::types::BridgeCandidateLiveState {
    pub(crate) fn from_core(live: bae_import::CandidateLiveState) -> Self {
        let identifying = live.identifying().map(crate::types::BridgeIdentifyingStep::from_core);
        let reading_pick = live.reading_pick();
        let bae_import::CandidateLiveState { facts, actions, standing } = live;
        // What identification is doing reaches the UI as what the row and
        // the pane show of it, and as the actions the row offers.
        let bae_import::TriageRuntimeFacts { identification: _, import } = facts;
        Self {
            identifying,
            reading_pick,
            import: import.map(crate::types::BridgeImportStep::from_core),
            actions: actions
                .into_iter()
                .map(crate::types::BridgeCandidateAction::from_core)
                .collect(),
            badge: standing
                .as_ref()
                .and_then(bae_import::PendingStanding::badge)
                .map(crate::types::BridgeRowBadge::from_core),
            standing: standing.map(crate::types::BridgePendingStanding::from_core),
        }
    }
}

impl crate::types::BridgeRowBadge {
    fn from_core(badge: bae_import::PendingBadge) -> Self {
        Self {
            says: crate::types::BridgePendingBadge::from_core(badge),
            tone: crate::types::BridgeBadgeTone::from_core(badge.tone()),
        }
    }
}

mirror_enum! {
    crate::types::BridgePendingBadge = bae_import::PendingBadge,
    from_core: fn,
    variants: {
        NeedsYou { reason: (crate::types::BridgeNeedsYouReason) },
        LookupError,
        Error,
        ImportError,
    },
}

mirror_enum! {
    crate::types::BridgeBadgeTone = bae_import::BadgeTone,
    from_core: fn,
    variants: { Attention, Failure },
}

mirror_enum! {
    crate::types::BridgeIdentifyingStep = bae_import::IdentifyingStep,
    from_core: fn,
    variants: { Queued, Running },
}

// Not a copy: audio no CD holds crosses with the rate its files state, which
// only the pane has at hand.
impl crate::types::BridgeFolderCheck {
    /// A check with the rate of the audio's files.
    pub(crate) fn with_rate(check: bae_identify::FolderCheck, rate: Option<u32>) -> Self {
        use bae_identify::FolderCheck;
        match check {
            FolderCheck::SourceTracksUnknown => Self::SourceTracksUnknown,
            FolderCheck::MediumDisagrees { folder } => Self::MediumDisagrees {
                folder: match folder {
                    bae_identify::MediumConflict::CdRip => {
                        crate::types::BridgeMediumConflict::CdRip
                    }
                    bae_identify::MediumConflict::NotCdAudio => {
                        crate::types::BridgeMediumConflict::NotCdAudio { sample_rate_hz: rate }
                    }
                },
            },
        }
    }
}

// ── The paged list ─────────────────────────────────────────────────────────

mirror_struct! {
    crate::types::BridgeTriageMetadataSummary = bae_import::triage::TriageMetadataSummary,
    from_core: fn,
    fields: {
        album_title,
        album_artists,
    },
}

mirror_enum! {
    crate::types::BridgeImportListOrder = bae_import::ImportListOrder,
    into_core: fn,
    variants: { NewestFirst, OldestFirst, PathAscending, PathDescending },
}

mirror_enum! {
    crate::types::BridgePendingFilter = bae_import::PendingFilter,
    from_core: fn,
    into_core: fn,
    variants: { All, NeedsYou, InProgress, Identified, Unmatched, NotLookedUp },
}

mirror_struct! {
    crate::types::BridgePendingFilterEntry = bae_import::PendingFilterEntry,
    from_core: fn,
    fields: {
        filter: (crate::types::BridgePendingFilter),
        count,
        selectable,
    },
}

mirror_struct! {
    crate::types::BridgeImportListNarrowing = bae_import::ImportListNarrowing,
    from_core: fn,
    fields: {
        tab: (crate::types::BridgeTriageTab),
        filter_text,
        pending_filter: (crate::types::BridgePendingFilter),
    },
}

mirror_struct! {
    crate::types::BridgeImportListView = bae_import::ImportListView,
    into_core: pub(crate) fn,
    fields: {
        tab: (crate::types::BridgeTriageTab),
        filter_text,
        pending_filter: (crate::types::BridgePendingFilter),
        order: (crate::types::BridgeImportListOrder),
    },
}

impl crate::types::BridgeImportListItem {
    pub(crate) fn from_core(item: bae_import::ImportListItem) -> Self {
        let stable_key = item.stable_key();
        match item {
            bae_import::ImportListItem::GroupHeader {
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
                watched_folder_path: coven::path_bytes(&watched_folder_path),
                expanded,
                entry_count,
            },
            bae_import::ImportListItem::Candidate { row, live, header_position } => {
                Self::Candidate {
                    stable_key,
                    row: crate::types::BridgeTriageRow::from_core(row, live),
                    header_position,
                }
            }
            bae_import::ImportListItem::Invalid { candidate, selected } => Self::Invalid {
                stable_key,
                invalid_candidate: crate::types::BridgeInvalidCandidate::from_core(candidate),
                selected,
            },
        }
    }
}

impl crate::types::BridgeActiveFolderScan {
    fn from_core(scan: bae_import::ActiveFolderScan) -> Self {
        let bae_import::ActiveFolderScan { watched_folder_path, watched_folder_name } = scan;
        Self { watched_folder_path: coven::path_bytes(&watched_folder_path), watched_folder_name }
    }
}

mirror_struct! {
    crate::types::BridgeFolderScanActivity = bae_import::FolderScanActivity,
    from_core: fn,
    fields: {
        folders: (each crate::types::BridgeActiveFolderScan),
    },
}

mirror_struct! {
    crate::types::BridgeNarrowedCount = bae_import::NarrowedCount,
    from_core: fn,
    fields: { shown, total },
}

mirror_struct! {
    crate::types::BridgeImportCounts = bae_import::ImportCounts,
    from_core: pub(crate) fn,
    fields: { identified, admitted, imports_in_flight },
}

mirror_struct! {
    crate::types::BridgeFolderScans = bae_import::FolderScanProgress,
    from_core: pub(crate) fn,
    fields: {
        statuses: (each crate::types::BridgeWatchedFolderScanStatus),
        activity: (opt crate::types::BridgeFolderScanActivity),
    },
}

impl crate::types::BridgeImportQueueSummary {
    fn from_core(summary: bae_import::ImportQueueSummary) -> Self {
        let bae_import::ImportQueueSummary {
            watched_folders,
            has_groups,
            narrowed,
            narrowing,
            opened_at,
        } = summary;
        Self {
            watched_folders: watched_folders
                .into_iter()
                .map(crate::types::BridgeWatchedFolder::from_core)
                .collect(),
            has_groups,
            narrowed: narrowed.map(crate::types::BridgeNarrowedCount::from_core),
            narrowing: crate::types::BridgeImportListNarrowing::from_core(narrowing),
            opened_at,
        }
    }
}

mirror_struct! {
    crate::types::BridgeImportCandidateListLocation
        = bae_import::ImportCandidateListLocation,
    from_core: pub(crate) fn,
    fields: {
        stable_key,
        tab: (crate::types::BridgeTriageTab),
        group_key: (opt crate::types::BridgeFolderReleaseDecisionKey),
        visible_position,
    },
}

impl crate::types::BridgeSelectionSummary {
    pub(super) fn from_core(summary: bae_import::selection::SelectionSummary) -> Self {
        let bae_import::selection::SelectionSummary { count, single, offers } = summary;
        Self {
            count,
            single: single.map(crate::types::BridgeSingleSelection::from_core),
            offers: offers.into_iter().map(crate::types::BridgeSelectionOffer::from_core).collect(),
        }
    }
}

impl crate::types::BridgeImportListSnapshot {
    pub(crate) fn from_core(snapshot: bae_import::ImportListSnapshot) -> Self {
        let bae_import::ImportListSnapshot {
            windows,
            total_count,
            summary,
            pending_filter_entries,
            selection_revision,
            selection,
            view_revision,
            cause,
        } = snapshot;
        Self {
            windows: windows
                .into_iter()
                .map(crate::types::BridgeImportListWindow::from_core)
                .collect(),
            total_count,
            summary: crate::types::BridgeImportQueueSummary::from_core(summary),
            pending_filter_entries: pending_filter_entries
                .into_iter()
                .map(crate::types::BridgePendingFilterEntry::from_core)
                .collect(),
            selection_revision,
            selection: crate::types::BridgeSelectionSummary::from_core(selection),
            view_revision,
            cause: crate::types::BridgeLiveQueryCause::from_core(cause),
        }
    }
}

impl crate::types::BridgeCandidatePanePlacement {
    /// Not a copy: the folder check crosses with `rate`, the rate of the
    /// audio's files.
    fn from_core(placement: bae_import::CandidatePanePlacement, rate: Option<u32>) -> Self {
        let records = |records: Vec<bae_catalogs::records::ReleaseRecord>| {
            records.into_iter().map(crate::types::BridgeReleaseRecord::from_core).collect()
        };
        match placement {
            bae_import::CandidatePanePlacement::Pending { folder_check, records: pending } => {
                Self::Pending {
                    folder_check: folder_check
                        .map(|check| crate::types::BridgeFolderCheck::with_rate(check, rate)),
                    records: records(pending),
                }
            }
            bae_import::CandidatePanePlacement::Skipped { records: skipped } => {
                Self::Skipped { records: records(skipped) }
            }
            bae_import::CandidatePanePlacement::Done => Self::Done,
        }
    }
}

impl crate::types::BridgeImportCandidateDetail {
    pub(crate) fn from_core(detail: bae_import::ImportCandidateDetail) -> Self {
        let is_added = detail.is_added();
        let bae_import::ImportCandidateDetail {
            candidate,
            actionable,
            skipped,
            resumed_identify_state,
            placement,
            live,
            import_status,
            // Only the automation interface lists the file of each clue.
            file_evidence: _,
            signals,
            stage,
        } = detail;
        let rate = resumed_identify_state.rate_ruling_out_cd();
        Self {
            candidate: crate::types::BridgeFolderCandidate::from_core(candidate, skipped, is_added),
            actionable,
            resumed_identify_state: crate::types::BridgeIdentifyState::from_core(
                resumed_identify_state,
            ),
            placement: crate::types::BridgeCandidatePanePlacement::from_core(placement, rate),
            live: crate::types::BridgeCandidateLiveState::from_core(live),
            import_status: import_status.map(crate::types::BridgeCandidateImportStatus::from_core),
            signals: signals.map(crate::types::BridgeSignals::from_core),
            stage: crate::types::BridgeCandidateStage::from_core(stage),
        }
    }
}

impl crate::types::BridgeCandidateStage {
    fn from_core(stage: bae_import::CandidateStage) -> Self {
        match stage {
            bae_import::CandidateStage::Drafting(draft) => {
                Self::Drafting { draft: crate::types::BridgeCandidateDraftPane::from_core(*draft) }
            }
            bae_import::CandidateStage::Imported { release, folder } => Self::Imported {
                release: crate::types::BridgeImportedRelease::from_core(release),
                folder: crate::types::BridgeFolderFiles::from_core(folder),
            },
        }
    }
}

impl crate::types::BridgeCandidateDraftPane {
    fn from_core(draft: bae_import::CandidateDraftPane) -> Self {
        let bae_import::CandidateDraftPane {
            picked_library_status,
            album,
            metadata_draft,
            artist_resolutions,
            metadata_draft_is_blank,
            metadata_provenance,
            release_link,
            metadata_author,
            mapping,
            cover,
            remote_covers,
            // Changed through `edit_candidate_lookup_choices`; nothing on the
            // pane reads it back.
            lookup_choices: _,
            failure,
            session,
        } = draft;
        Self {
            remote_covers: remote_covers
                .into_iter()
                .map(crate::types::BridgeRemoteCover::from_core)
                .collect(),
            picked_library_status: picked_library_status
                .map(crate::types::BridgeLibraryStatus::from_core),
            album: crate::types::BridgeDraftAlbum::from_core(album),
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
            mapping: crate::types::BridgeMappingTable::from_core(mapping),
            cover: cover.map(crate::types::BridgeCoverChoice::from_core),
            failure: failure
                .map(|failure| crate::types::BridgeError::from_core(failure.reason.ui_error())),
            session: crate::types::BridgeCandidateSession::from_core(session),
        }
    }
}

mirror_enum! {
    crate::types::BridgeSelectionChange = bae_import::selection::SelectionChange,
    into_core: pub(crate) fn,
    variants: {
        Replace { keys: (each into) },
        Toggle { add: (each into), remove: (each into) },
        Extend { from: (into), to: (into) },
    },
}
