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
    variants: {
        NotScanned,
        Scanning { on_network_volume },
        Complete { on_network_volume },
        Failed { error, on_network_volume },
        NotWatched { reason, on_network_volume },
    },
}

impl crate::types::BridgeWatchedFolderScanStatus {
    pub(super) fn from_core(status: bae_import::WatchedFolderScanStatus) -> Self {
        let bae_import::WatchedFolderScanStatus { folder, status } = status;
        Self {
            folder: crate::types::BridgeWatchedFolder::from_core(folder),
            status: crate::types::BridgeFolderScanStatus::from_core(status),
        }
    }
}

impl crate::types::BridgeFolderCandidate {
    pub(super) fn from_core(
        candidate: bae_import::FolderCandidate,
        files: crate::types::BridgeCandidateFiles,
        track_count: u32,
        skipped: bool,
    ) -> Self {
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
            files,
            track_count,
            skipped,
        }
    }
}

impl crate::types::BridgeInvalidCandidate {
    pub(crate) fn from_core(row: bae_import::triage::InvalidRow) -> Self {
        let bae_import::triage::InvalidRow {
            candidate_key,
            folder,
            folder_name,
            watched_folder_path,
            separable,
            reason,
            tags,
            cover,
        } = row;
        crate::types::BridgeInvalidCandidate {
            candidate_key: candidate_key.into(),
            folder_path: coven::path_bytes(&folder),
            source_folder_name: folder_name,
            watched_folder_path: coven::path_bytes(&watched_folder_path),
            separable,
            reason: crate::types::BridgeInvalidReason::from_core(reason),
            tags: tags.map(crate::types::BridgeSkippedTags::from_core),
            cover: cover.map(crate::types::BridgeCoverImageSource::from_core),
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
    pub(crate) fn from_core(view: bae_import::CandidateRuntimeView) -> Self {
        let bae_import::CandidateRuntimeView { identify_state, in_flight: runtime } = view;
        Self {
            identify_state: crate::types::BridgeShownIdentifyState::from_core(identify_state),
            in_flight: runtime.map(crate::types::BridgeCandidateInFlight::from_core),
        }
    }
}

impl crate::types::BridgeCandidateInFlight {
    fn from_core(runtime: bae_import::CandidateRuntimeSnapshot) -> Self {
        // A failed write, and whether an Import was asked of the answer,
        // reach the UI through the row's live state.
        let signals = match runtime.identification_in_flight() {
            Some(bae_import::IdentificationInFlight::Run(published)) => {
                published.signals.map(crate::types::BridgeSignals::from_core)
            }
            Some(bae_import::IdentificationInFlight::Queued) | None => None,
        };
        // The pane reads what identification is doing from the shown
        // identify state.
        let bae_import::CandidateRuntimeSnapshot { identification: _, import, search } = runtime;
        Self {
            import: import.map(crate::types::BridgeImportInFlight::from_core),
            search: search.map(crate::types::BridgeCandidateSearch::from_core),
            signals,
        }
    }
}

impl crate::types::BridgeShownIdentifyState {
    fn from_core(state: bae_import::ShownIdentifyState) -> Self {
        let state = match state {
            bae_import::ShownIdentifyState::Queued => return Self::Queued,
            bae_import::ShownIdentifyState::Run(published) => {
                crate::types::BridgeIdentifyState::from_published(published)
            }
            bae_import::ShownIdentifyState::Stored(bae_import::StoredVerdict {
                view,
                not_cd_rate,
            }) => crate::types::BridgeIdentifyState::from_view(view, not_cd_rate),
            bae_import::ShownIdentifyState::Idle => crate::types::BridgeIdentifyState::Idle,
        };
        Self::State { state }
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
        } = row;
        crate::types::BridgeTriageRow {
            candidate_key: candidate_key.into(),
            folder_name,
            watched_folder_path: coven::path_bytes(&watched_folder_path),
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
    into_core: pub(crate) fn,
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
        NotFound,
        NothingToLookUp,
    },
}

impl crate::types::BridgeCandidateLiveState {
    pub(crate) fn from_core(live: bae_import::CandidateLiveState) -> Self {
        let identifying = live.identifying().map(crate::types::BridgeIdentifyingStep::from_core);
        let offers = live.offers();
        let bae_import::CandidateLiveState { facts, actions: _, standing } = live;
        // What identification is doing reaches the UI as what the row and
        // the pane show of it, and as the offers its own menu shows.
        let bae_import::TriageRuntimeFacts { identification: _, import } = facts;
        Self {
            identifying,
            import: import.map(crate::types::BridgeImportStep::from_core),
            offers: offers.into_iter().map(crate::types::BridgeSelectionOffer::from_core).collect(),
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
        album_artists: (crate::types::BridgeCreditLine),
    },
}

mirror_enum! {
    crate::types::BridgeImportListOrder = bae_import::ImportListOrder,
    into_core: pub(crate) fn,
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
    from_core: pub(crate) fn,
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

impl crate::types::BridgeImportGroupHeader {
    pub(crate) fn from_core(header: bae_import::ImportGroupHeader) -> Self {
        let stable_key = header.stable_key();
        let shows_no_member = header.shows_no_member();
        let bae_import::ImportGroupHeader { group, watched_folder_path, expanded, entry_count } =
            header;
        Self {
            stable_key,
            group: crate::types::BridgeTriageGroup {
                key: crate::types::BridgeFolderReleaseDecisionKey::from_core(group.key),
                name: group.name,
                combinable: group.combinable,
            },
            watched_folder_path: coven::path_bytes(&watched_folder_path),
            expanded,
            entry_count,
            shows_no_member,
        }
    }
}

impl crate::types::BridgeImportListItem {
    pub(crate) fn from_core(item: bae_import::ImportListItem) -> Self {
        let stable_key = item.stable_key();
        match item {
            bae_import::ImportListItem::GroupHeader(header) => Self::GroupHeader {
                header: crate::types::BridgeImportGroupHeader::from_core(header),
            },
            bae_import::ImportListItem::Candidate { row, live, group } => Self::Candidate {
                stable_key,
                row: crate::types::BridgeTriageRow::from_core(row, live),
                group: group.map(|bae_import::ImportGroupMember { header, last }| {
                    crate::types::BridgeImportGroupMember {
                        header: crate::types::BridgeImportGroupHeader::from_core(header),
                        last,
                    }
                }),
            },
            bae_import::ImportListItem::Invalid { row } => Self::Invalid {
                stable_key,
                invalid_candidate: crate::types::BridgeInvalidCandidate::from_core(row),
            },
        }
    }
}

impl crate::types::BridgeScanningFolder {
    pub(crate) fn from_core(scan: bae_import::ScanningFolder) -> Self {
        let bae_import::ScanningFolder { folder, found } = scan;
        Self { folder: crate::types::BridgeWatchedFolder::from_core(folder), found }
    }
}

mirror_struct! {
    crate::types::BridgeNarrowedCount = bae_import::NarrowedCount,
    from_core: pub(crate) fn,
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
        has_problem,
        scanning,
        alerts: (each crate::types::BridgeFolderScanAlert),
    },
}

mirror_enum! {
    crate::types::BridgeFolderScanAlertKind = bae_import::FolderScanAlertKind,
    from_core: fn,
    into_core: pub(super) fn,
    variants: { Failed, NotWatched },
}

impl crate::types::BridgeFolderScanAlert {
    fn from_core(alert: bae_import::FolderScanAlert) -> Self {
        let bae_import::FolderScanAlert { folder, kind, detail } = alert;
        Self {
            folder: crate::types::BridgeWatchedFolder::from_core(folder),
            kind: crate::types::BridgeFolderScanAlertKind::from_core(kind),
            detail,
        }
    }
}

impl crate::types::BridgeImportQueueSummary {
    pub(crate) fn from_core(summary: bae_import::ImportQueueSummary) -> Self {
        let bae_import::ImportQueueSummary { has_watched_folders, has_groups, narrowing } = summary;
        Self {
            has_watched_folders,
            has_groups,
            narrowing: crate::types::BridgeImportListNarrowing::from_core(narrowing),
        }
    }
}

impl crate::types::BridgeImportCandidateListLocation {
    pub(crate) fn from_core(location: bae_import::ImportCandidateListLocation) -> Self {
        let bae_import::ImportCandidateListLocation { tab, group_key, .. } = location;
        Self {
            tab: crate::types::BridgeTriageTab::from_core(tab),
            group_key: group_key.map(crate::types::BridgeFolderReleaseDecisionKey::from_core),
        }
    }
}

impl crate::types::BridgeImportListOpening {
    pub(crate) fn from_core(opening: bae_import::ImportListOpening) -> Self {
        match opening {
            bae_import::ImportListOpening::Top => Self::Top,
            bae_import::ImportListOpening::At { position } => Self::At { position },
        }
    }
}

impl crate::types::BridgeImportListViewChange {
    pub(crate) fn from_core(change: bae_import::ImportListViewChange) -> Self {
        Self {
            opening: crate::types::BridgeImportListOpening::from_core(change.opening),
            left: change.left.into_iter().map(Vec::<u8>::from).collect(),
        }
    }
}

impl crate::types::BridgeSelectionSummary {
    pub(crate) fn from_core(summary: bae_import::selection::SelectionSummary) -> Self {
        let bae_import::selection::SelectionSummary { count, single, offers } = summary;
        Self {
            count,
            single: single.map(crate::types::BridgeSingleSelection::from_core),
            offers: offers.into_iter().map(crate::types::BridgeSelectionOffer::from_core).collect(),
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
        let bae_import::ImportCandidateDetail {
            candidate,
            files,
            part_count,
            actionable,
            skipped,
            // The pane reads the identify state it shows from the
            // candidate's runtime read.
            resumed_identify_state: _,
            placement,
            live,
            import_status,
            // Only the automation interface lists the file of each clue.
            file_evidence: _,
            signals,
            stage,
        } = detail;
        let rate = signals.as_ref().and_then(|signals| signals.origin.not_cd_rate);
        let bae_import::PaneFiles { files, cover_files, images, track_count, source_audio } = files;
        let files = crate::types::BridgeCandidateFiles::from_core(
            candidate.files.file_metadata_identity(),
            files,
            cover_files,
            source_audio,
        );
        Self {
            candidate: crate::types::BridgeFolderCandidate::from_core(
                candidate,
                files,
                track_count,
                skipped,
            ),
            part_count,
            actionable,
            placement: crate::types::BridgeCandidatePanePlacement::from_core(placement, rate),
            live: crate::types::BridgeCandidateLiveState::from_core(live),
            import_status: import_status.map(crate::types::BridgeCandidateImportStatus::from_core),
            signals: signals.map(crate::types::BridgeSignals::from_core),
            stage: crate::types::BridgeCandidateStage::from_core(stage, images),
        }
    }
}

impl crate::types::BridgeCandidateStage {
    /// Builds the pane's stage. An imported candidate's gallery shows
    /// `images`, the images in the candidate's folder.
    fn from_core(stage: bae_import::CandidateStage, images: Vec<bae_import::MappingImage>) -> Self {
        match stage {
            bae_import::CandidateStage::Drafting(draft) => {
                Self::Drafting { draft: crate::types::BridgeCandidateDraftPane::from_core(*draft) }
            }
            bae_import::CandidateStage::Imported { release } => Self::Imported {
                release: crate::types::BridgeImportedRelease::from_core(release),
                images: images
                    .into_iter()
                    .map(crate::types::BridgeMappingImage::from_core)
                    .collect(),
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
            album_artist_standing,
            metadata_draft_is_blank,
            metadata_provenance,
            release_link,
            mapping,
            cover,
            // Changed through `edit_candidate_lookup_choices`; nothing on the
            // pane reads it back.
            lookup_choices: _,
            failure,
            session,
        } = draft;
        Self {
            picked_library_status: picked_library_status
                .map(crate::types::BridgeLibraryStatus::from_core),
            album: crate::types::BridgeDraftAlbum::from_core(album),
            metadata_draft: crate::types::BridgeRawReleaseEdit::from_core(metadata_draft),
            album_artist_standing: album_artist_standing
                .map(crate::types::BridgeArtistFieldStanding::from_core),
            metadata_draft_is_blank,
            metadata_provenance: metadata_provenance
                .map(crate::types::BridgeMetadataProvenance::from_core),
            release_link: release_link.map(crate::types::BridgeReleaseLink::from_core),
            mapping: crate::types::BridgeMappingTable::from_core(mapping),
            cover: cover.map(crate::types::BridgeCoverChoice::from_core),
            failure: failure
                .map(|failure| crate::types::BridgeError::from_core(failure.reason.ui_error())),
            session: crate::types::BridgeCandidateSession::from_core(session),
        }
    }
}
