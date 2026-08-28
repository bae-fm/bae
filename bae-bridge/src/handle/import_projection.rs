#[cfg(feature = "desktop")]
impl crate::types::BridgeFolderReleaseDecisionKey {
    pub(super) fn from_core(key: bae_core::import::FolderReleaseDecisionKey) -> Self {
        Self {
            watched_folder_path: key.watched_folder_path,
            relative_folder_path: key.relative_folder_path,
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::FolderReleaseDecisionKey {
        bae_core::import::FolderReleaseDecisionKey {
            watched_folder_path: self.watched_folder_path,
            relative_folder_path: self.relative_folder_path,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeFolderReleaseDecision {
    pub(super) fn from_core(decision: bae_core::import::FolderReleaseDecision) -> Self {
        match decision {
            bae_core::import::FolderReleaseDecision::CombineAsOneRelease => {
                Self::CombineAsOneRelease
            }
            bae_core::import::FolderReleaseDecision::KeepAsSeparateReleases => {
                Self::KeepAsSeparateReleases
            }
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::FolderReleaseDecision {
        match self {
            Self::CombineAsOneRelease => {
                bae_core::import::FolderReleaseDecision::CombineAsOneRelease
            }
            Self::KeepAsSeparateReleases => {
                bae_core::import::FolderReleaseDecision::KeepAsSeparateReleases
            }
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeResolvedFolderReleaseBoundary {
    pub(super) fn from_core(boundary: bae_core::import::ResolvedFolderReleaseBoundary) -> Self {
        Self {
            key: crate::types::BridgeFolderReleaseDecisionKey::from_core(boundary.key),
            decision: crate::types::BridgeFolderReleaseDecision::from_core(boundary.decision),
            name: boundary.name,
            display_path: boundary.display_path,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeWatchedFolderScanStatus {
    pub(super) fn from_core(status: bae_core::import::WatchedFolderScanStatus) -> Self {
        Self {
            watched_folder_path: status.watched_folder_path,
            watched_folder_name: status.watched_folder_name,
            on_network_volume: status.on_network_volume,
            status: match status.status {
                bae_core::import::FolderScanStatus::Scanning => {
                    crate::types::BridgeFolderScanStatus::Scanning
                }
                bae_core::import::FolderScanStatus::Complete => {
                    crate::types::BridgeFolderScanStatus::Complete
                }
                bae_core::import::FolderScanStatus::Failed { error } => {
                    crate::types::BridgeFolderScanStatus::Failed { error }
                }
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeFolderCandidate {
    pub(super) fn from_core(
        candidate: bae_core::import::FolderCandidate,
        skipped: bool,
        is_added: bool,
    ) -> Self {
        // `track_count()` borrows `&candidate`; compute it before the move.
        let track_count = candidate.track_count();
        let bae_core::import::FolderCandidate {
            path,
            name,
            files,
            watched_folder_path,
            ..
        } = candidate;
        crate::types::BridgeFolderCandidate {
            folder_path: path.to_string_lossy().to_string(),
            source_folder_name: name,
            watched_folder_path,
            files: crate::types::BridgeCandidateFiles::from_core(files),
            track_count,
            skipped,
            is_added,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeInvalidCandidate {
    pub(super) fn from_core(candidate: bae_core::import::InvalidCandidate) -> Self {
        let bae_core::import::InvalidCandidate {
            path,
            name,
            watched_folder_path,
            display_path,
            resolved_boundaries,
            reason,
        } = candidate;
        crate::types::BridgeInvalidCandidate {
            folder_path: path.to_string_lossy().to_string(),
            source_folder_name: name,
            watched_folder_path,
            display_path,
            resolved_boundaries: resolved_boundaries
                .into_iter()
                .map(crate::types::BridgeResolvedFolderReleaseBoundary::from_core)
                .collect(),
            reason: crate::types::BridgeInvalidReason::from_core(reason),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeCandidateRuntimeSnapshot {
    pub(crate) fn from_core(runtime: bae_core::import::CandidateRuntimeSnapshot) -> Self {
        let bae_core::import::CandidateRuntimeSnapshot { identify, import } = runtime;
        crate::types::BridgeCandidateRuntimeSnapshot {
            signals_toolbar: crate::types::BridgeSignalsToolbar::from_core(
                identify
                    .as_ref()
                    .map(bae_core::identify::IdentifyState::toolbar)
                    .unwrap_or_default(),
            ),
            identify_state: crate::types::BridgeIdentifyState::from_core(
                identify.unwrap_or(bae_core::identify::IdentifyState::Idle),
            ),
            import: import.map(crate::types::BridgeImportInFlight::from_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeImportInFlight {
    fn from_core(import: bae_core::import::ImportInFlight) -> Self {
        crate::types::BridgeImportInFlight {
            progress_percent: import.progress_percent,
            step: import.step.map(crate::types::BridgeImportStep::from_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeCandidateRuntimeChange {
    pub(super) fn from_core(change: bae_core::import::CandidateRuntimeChange) -> Self {
        match change {
            bae_core::import::CandidateRuntimeChange::Updated { key, runtime } => Self::Updated {
                key,
                runtime: crate::types::BridgeCandidateRuntimeSnapshot::from_core(runtime),
            },
            bae_core::import::CandidateRuntimeChange::Removed { key } => Self::Removed { key },
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

#[cfg(feature = "desktop")]
impl crate::types::BridgeTriageImportStatus {
    pub(super) fn from_core(status: bae_core::import::triage::TriageImportStatus) -> Self {
        match status {
            bae_core::import::triage::TriageImportStatus::Importing => Self::Importing,
            bae_core::import::triage::TriageImportStatus::Complete { release } => Self::Complete {
                release_id: release.release_id,
                album_id: release.album_id,
            },
            bae_core::import::triage::TriageImportStatus::Error { error } => Self::Error {
                error: crate::types::BridgeError::from_core(bae_core::ui::UiError::import(error)),
            },
        }
    }
}

// ── Sidebar triage ─────────────────────────────────────────────────────────
//
// A mirror, variant for variant. Every decision behind these values was made in
// `bae_core::import::triage`.

#[cfg(feature = "desktop")]
impl crate::types::BridgeTriageRow {
    pub(crate) fn from_core(row: bae_core::import::TriageRow) -> Self {
        let bae_core::import::TriageRow {
            candidate_key,
            folder_name,
            watched_folder_path,
            display_path,
            resolved_boundaries,
            combine_ancestor_key,
            actionable,
            placement,
            skip_action,
            matched,
            selectable,
            import_status,
            metadata_provenance,
        } = row;
        crate::types::BridgeTriageRow {
            candidate_key,
            folder_name,
            watched_folder_path,
            display_path,
            resolved_boundaries: resolved_boundaries
                .into_iter()
                .map(crate::types::BridgeResolvedFolderReleaseBoundary::from_core)
                .collect(),
            combine_ancestor_key: combine_ancestor_key
                .map(crate::types::BridgeFolderReleaseDecisionKey::from_core),
            actionable,
            placement: crate::types::BridgeTriagePlacement::from_core(placement),
            skip_action: skip_action.map(crate::types::BridgeTriageSkipAction::from_core),
            matched: matched.map(crate::types::BridgeMatchedRelease::from_core),
            selectable,
            import_status: import_status.map(crate::types::BridgeTriageImportStatus::from_core),
            metadata_provenance: metadata_provenance
                .map(crate::types::BridgeMetadataProvenance::from_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeTriageTab {
    pub(super) fn from_core(tab: bae_core::import::TriageTab) -> Self {
        match tab {
            bae_core::import::TriageTab::Pending => Self::Pending,
            bae_core::import::TriageTab::Done => Self::Done,
            bae_core::import::TriageTab::Skipped => Self::Skipped,
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::TriageTab {
        match self {
            Self::Pending => bae_core::import::TriageTab::Pending,
            Self::Done => bae_core::import::TriageTab::Done,
            Self::Skipped => bae_core::import::TriageTab::Skipped,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeTriageSkipAction {
    pub(crate) fn from_core(action: bae_core::import::TriageSkipAction) -> Self {
        match action {
            bae_core::import::TriageSkipAction::Skip => Self::Skip,
            bae_core::import::TriageSkipAction::Unskip => Self::Unskip,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeTriagePlacement {
    pub(crate) fn from_core(placement: bae_core::import::TriagePlacement) -> Self {
        use bae_core::import::TriagePlacement as P;
        match placement {
            P::Pending => Self::Pending,
            P::Ready => Self::Ready,
            P::NeedsYou { group, reason } => Self::NeedsYou {
                group: crate::types::BridgeNeedsYouGroup::from_core(group),
                reason: crate::types::BridgeNeedsYouReason::from_core(reason),
            },
            P::Importing => Self::Importing,
            P::Failed => Self::Failed,
            P::Done => Self::Done,
            P::Skipped => Self::Skipped,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeNeedsYouGroup {
    pub(crate) fn from_core(group: bae_core::import::NeedsYouGroup) -> Self {
        use bae_core::import::NeedsYouGroup as G;
        match group {
            G::PickAPressing => Self::PickAPressing,
            G::CountsOrLengthsDisagree => Self::CountsOrLengthsDisagree,
            G::AlreadyInLibrary => Self::AlreadyInLibrary,
            G::NoMatch => Self::NoMatch,
            G::StillIdentifying => Self::StillIdentifying,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeNeedsYouReason {
    pub(crate) fn from_core(reason: bae_core::import::NeedsYouReason) -> Self {
        use bae_core::import::NeedsYouReason as R;
        match reason {
            R::Disagreement(needs_you) => Self::Disagreement {
                disagreement: crate::types::BridgeNeedsYou::from_core(needs_you),
            },
            R::StillIdentifying { phase } => Self::StillIdentifying {
                phase: crate::types::BridgeIdentifyPhase::from_core(phase),
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeIdentifyPhase {
    pub(crate) fn from_core(phase: bae_core::import::IdentifyPhase) -> Self {
        use bae_core::import::IdentifyPhase as P;
        match phase {
            P::Queued => Self::Queued,
            P::Running => Self::Running,
            P::NoAnswer => Self::NoAnswer,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeNeedsYou {
    pub(crate) fn from_core(needs_you: bae_core::identify::NeedsYou) -> Self {
        use bae_core::identify::NeedsYou as N;
        match needs_you {
            N::AlreadyInLibrary => Self::AlreadyInLibrary,
            N::SeveralMatches { count } => Self::SeveralMatches { count },
            N::NoMatch => Self::NoMatch,
            N::NothingToLookUp => Self::NothingToLookUp,
            N::TrackCountDisagrees { local, source } => Self::TrackCountDisagrees { local, source },
            N::DurationsDisagree {
                probed_ms,
                source_ms,
                tolerance_ms,
            } => Self::DurationsDisagree {
                probed_ms,
                source_ms,
                tolerance_ms,
            },
            N::SourceLengthsUnknown => Self::SourceLengthsUnknown,
            N::LocalDurationUnknown => Self::LocalDurationUnknown,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeMatchedRelease {
    pub(crate) fn from_core(matched: bae_core::import::MatchedRelease) -> Self {
        let bae_core::import::MatchedRelease {
            release_id,
            title,
            artist,
            pressing,
            cover_thumbnail_url,
            evidence,
        } = matched;
        let bae_core::import::MatchEvidence { source, signal } = evidence;
        crate::types::BridgeMatchedRelease {
            release_id,
            title,
            artist,
            pressing: pressing.map(|pressing| {
                let bae_core::import::MatchedPressing {
                    year,
                    format,
                    track_count,
                } = pressing;
                crate::types::BridgeMatchedPressing {
                    year,
                    format,
                    track_count,
                }
            }),
            cover_thumbnail_url,
            evidence: crate::types::BridgeMatchEvidence {
                source: crate::types::BridgeMetadataSource::from_core(source),
                signal: signal.map(crate::types::BridgeMatchedSignal::from_core),
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeMatchedSignal {
    pub(crate) fn from_core(signal: bae_core::import::MatchedSignal) -> Self {
        use bae_core::import::MatchedSignal as S;
        match signal {
            S::DiscId => Self::DiscId,
            S::Barcode => Self::Barcode,
        }
    }
}

// ── The paged list ─────────────────────────────────────────────────────────

#[cfg(feature = "desktop")]
impl crate::types::BridgeImportListView {
    pub(super) fn into_core(self) -> bae_core::import::ImportListView {
        bae_core::import::ImportListView {
            tab: self.tab.into_core(),
            filter_text: self.filter_text,
            collapsed_groups: self
                .collapsed_groups
                .into_iter()
                .map(crate::types::BridgeFolderReleaseDecisionKey::into_core)
                .collect(),
            order: match self.order {
                crate::types::BridgeImportListOrder::PathAscending => {
                    bae_core::import::ImportListOrder::PathAscending
                }
                crate::types::BridgeImportListOrder::PathDescending => {
                    bae_core::import::ImportListOrder::PathDescending
                }
            },
        }
    }
}

#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
impl crate::types::BridgeImportQueueSummary {
    pub(super) fn from_core(summary: bae_core::import::ImportQueueSummary) -> Self {
        let bae_core::import::ImportQueueSummary {
            counts,
            watched_folders,
            folder_scan_statuses,
            group_keys,
            ready,
            first_unidentified,
        } = summary;
        let bae_core::import::TriageTabCounts {
            pending,
            done,
            skipped,
        } = counts;
        Self {
            counts: crate::types::BridgeTriageTabCounts {
                pending,
                done,
                skipped,
            },
            watched_folders: watched_folders
                .into_iter()
                .map(crate::types::BridgeWatchedFolder::from_core)
                .collect(),
            folder_scan_statuses: folder_scan_statuses
                .into_iter()
                .map(crate::types::BridgeWatchedFolderScanStatus::from_core)
                .collect(),
            group_keys: group_keys
                .into_iter()
                .map(crate::types::BridgeFolderReleaseDecisionKey::from_core)
                .collect(),
            ready: ready
                .into_iter()
                .map(|row| crate::types::BridgeReadyRowRef {
                    candidate_key: row.candidate_key,
                    cover_thumbnail_url: row.cover_thumbnail_url,
                })
                .collect(),
            first_unidentified: first_unidentified
                .map(crate::types::BridgeFirstUnidentifiedRowRef::from_core),
        }
    }
}

impl crate::types::BridgeFirstUnidentifiedRowRef {
    pub(super) fn from_core(row: bae_core::import::FirstUnidentifiedRowRef) -> Self {
        Self {
            candidate_key: row.candidate_key,
            stable_key: row.stable_key,
            group_key: row
                .group_key
                .map(crate::types::BridgeFolderReleaseDecisionKey::from_core),
            visible_position: row.visible_position,
        }
    }
}

impl crate::types::BridgeImportCandidateListLocation {
    pub(super) fn from_core(location: bae_core::import::ImportCandidateListLocation) -> Self {
        Self {
            stable_key: location.stable_key,
            tab: crate::types::BridgeTriageTab::from_core(location.tab),
            group_key: location
                .group_key
                .map(crate::types::BridgeFolderReleaseDecisionKey::from_core),
            visible_position: location.visible_position,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeImportListSnapshot {
    pub(super) fn from_core(snapshot: bae_core::import::ImportListSnapshot) -> Self {
        Self {
            windows: snapshot
                .windows
                .into_iter()
                .map(|window| crate::types::BridgeImportListWindow {
                    window: crate::types::BridgeLibraryPageWindow::from_core(window.window),
                    items: window
                        .items
                        .into_iter()
                        .map(crate::types::BridgeImportListItem::from_core)
                        .collect(),
                })
                .collect(),
            total_count: snapshot.total_count,
            summary: crate::types::BridgeImportQueueSummary::from_core(snapshot.summary),
            request_revision: snapshot.request_revision,
            cause: crate::types::BridgeLiveQueryCause::from_core(snapshot.cause),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeImportCandidateDetail {
    pub(super) fn from_core(detail: bae_core::import::ImportCandidateDetail) -> Self {
        let bae_core::import::ImportCandidateDetail {
            candidate,
            actionable,
            skipped,
            is_added,
            resumed_identify_state,
            row,
            release,
            picked_library_status,
            file_evidence,
            metadata_draft,
            metadata_draft_is_blank,
            metadata_provenance,
            metadata_revision,
            initial_metadata_source,
            mapping,
            unprobed,
            cover,
            // Every cover the picker offers is already inside `release`, whose
            // `cover_art` is the same list.
            remote_covers: _,
            signals,
            failure,
        } = detail;
        Self {
            candidate: crate::types::BridgeFolderCandidate::from_core(candidate, skipped, is_added),
            actionable,
            resumed_identify_state: crate::types::BridgeIdentifyState::from_core(
                resumed_identify_state,
            ),
            row: crate::types::BridgeTriageRow::from_core(row),
            release: release.map(crate::types::BridgeReleaseDetail::from_core),
            picked_library_status: picked_library_status
                .map(crate::types::BridgeLibraryStatus::from_core),
            file_evidence: file_evidence
                .into_iter()
                .map(crate::types::BridgeFileEvidence::from_core)
                .collect(),
            metadata_draft: crate::types::BridgeRawReleaseEdit::from_core(metadata_draft),
            metadata_draft_is_blank,
            metadata_provenance: metadata_provenance
                .map(crate::types::BridgeMetadataProvenance::from_core),
            metadata_revision,
            initial_metadata_source: crate::types::BridgeDefaultImportMetadataSource::from_core(
                initial_metadata_source,
            ),
            mapping: crate::types::BridgeMappingTable::from_core(mapping),
            unprobed: unprobed
                .into_iter()
                .map(crate::types::BridgeAudioFile::from_core)
                .collect(),
            cover: cover.map(crate::types::BridgeCoverChoice::from_core),
            signals: signals.map(crate::types::BridgeSignals::from_core),
            failure: failure.map(|failure| crate::types::BridgeImportFailure {
                error: failure.error,
                failed_at: failure.failed_at.to_rfc3339(),
            }),
        }
    }
}
