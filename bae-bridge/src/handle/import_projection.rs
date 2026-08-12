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
impl crate::types::BridgeFolderReleaseBoundary {
    pub(super) fn from_core(boundary: bae_core::import::FolderReleaseBoundary) -> Self {
        Self {
            key: crate::types::BridgeFolderReleaseDecisionKey::from_core(boundary.key),
            name: boundary.name,
            display_path: boundary.display_path,
            shared_file_count: boundary.shared_file_count,
            tree_rows: boundary
                .tree_rows
                .into_iter()
                .map(|row| crate::types::BridgeFolderReleaseTreeRow {
                    name: row.name,
                    display_path: row.display_path,
                    depth: row.depth,
                    kind: match row.kind {
                        bae_core::import::FolderReleaseTreeRowKind::Folder => {
                            crate::types::BridgeFolderReleaseTreeRowKind::Folder
                        }
                        bae_core::import::FolderReleaseTreeRowKind::Candidate { summary } => {
                            crate::types::BridgeFolderReleaseTreeRowKind::Candidate {
                                track_count: summary.track_count,
                                format_label: summary.format_label,
                            }
                        }
                        bae_core::import::FolderReleaseTreeRowKind::Invalid { reason } => {
                            crate::types::BridgeFolderReleaseTreeRowKind::Invalid {
                                reason: crate::types::BridgeInvalidReason::from_core(reason),
                            }
                        }
                    },
                    decision_key: crate::types::BridgeFolderReleaseDecisionKey::from_core(
                        row.decision_key,
                    ),
                    ancestor_decision_keys: row
                        .ancestor_decision_keys
                        .into_iter()
                        .map(crate::types::BridgeFolderReleaseDecisionKey::from_core)
                        .collect(),
                })
                .collect(),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeWatchedFolderScanStatus {
    pub(super) fn from_core(status: bae_core::import::WatchedFolderScanStatus) -> Self {
        Self {
            watched_folder_path: status.watched_folder_path,
            watched_folder_name: status.watched_folder_name,
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
    pub(super) fn from_core(runtime: bae_core::import::CandidateRuntimeSnapshot) -> Self {
        let bae_core::import::CandidateRuntimeSnapshot {
            identify_state,
            toolbar,
            signals,
            import_status,
        } = runtime;
        crate::types::BridgeCandidateRuntimeSnapshot {
            identify_state: crate::types::BridgeIdentifyState::from_core(identify_state),
            signals_toolbar: crate::types::BridgeSignalsToolbar::from_core(toolbar),
            signals: signals.map(crate::types::BridgeSignals::from_core),
            import_status: import_status.map(crate::types::BridgeCandidateImportStatus::from_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeCandidateImportStatus {
    pub(super) fn from_core(status: bae_core::import::CandidateImportStatusSnapshot) -> Self {
        match status {
            bae_core::import::CandidateImportStatusSnapshot::Importing {
                progress_percent,
                step,
            } => crate::types::BridgeCandidateImportStatus::Importing {
                progress_percent,
                step: step.map(crate::types::BridgeImportStep::from_core),
            },
            bae_core::import::CandidateImportStatusSnapshot::Complete {
                release_id,
                album_id,
            } => crate::types::BridgeCandidateImportStatus::Complete {
                release_id,
                album_id,
            },
            bae_core::import::CandidateImportStatusSnapshot::Error { error } => {
                crate::types::BridgeCandidateImportStatus::Error {
                    error: crate::types::BridgeError::from_core(bae_core::ui::UiError::import(
                        error,
                    )),
                }
            }
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeFolderImportCandidateSnapshot {
    pub(super) fn from_core(snapshot: bae_core::import::FolderImportCandidateSnapshot) -> Self {
        let bae_core::import::FolderImportCandidateSnapshot {
            candidate,
            runtime,
            actionable,
            skipped,
            is_added,
        } = snapshot;
        crate::types::BridgeFolderImportCandidateSnapshot {
            candidate: crate::types::BridgeFolderCandidate::from_core(candidate, skipped, is_added),
            runtime: crate::types::BridgeCandidateRuntimeSnapshot::from_core(runtime),
            actionable,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeRuntimeImportCandidateSnapshot {
    pub(super) fn from_core(snapshot: bae_core::import::RuntimeImportCandidateSnapshot) -> Self {
        Self {
            key: snapshot.key,
            runtime: crate::types::BridgeCandidateRuntimeSnapshot::from_core(snapshot.runtime),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeImportCandidatesSnapshot {
    pub(super) fn from_core(snapshot: bae_core::import::ImportCandidatesSnapshot) -> Self {
        let bae_core::import::ImportCandidatesSnapshot {
            watched_folders,
            folder_candidates,
            runtime_candidates,
            invalid_candidates,
            boundaries,
            folder_scan_statuses,
        } = snapshot;
        crate::types::BridgeImportCandidatesSnapshot {
            watched_folders: watched_folders
                .into_iter()
                .map(crate::types::BridgeWatchedFolder::from_core)
                .collect(),
            folder_candidates: folder_candidates
                .into_iter()
                .map(crate::types::BridgeFolderImportCandidateSnapshot::from_core)
                .collect(),
            runtime_candidates: runtime_candidates
                .into_iter()
                .map(crate::types::BridgeRuntimeImportCandidateSnapshot::from_core)
                .collect(),
            invalid_candidates: invalid_candidates
                .into_iter()
                .map(crate::types::BridgeInvalidCandidate::from_core)
                .collect(),
            boundaries: boundaries
                .into_iter()
                .map(crate::types::BridgeFolderReleaseBoundary::from_core)
                .collect(),
            folder_scan_statuses: folder_scan_statuses
                .into_iter()
                .map(crate::types::BridgeWatchedFolderScanStatus::from_core)
                .collect(),
        }
    }
}

// ── Sidebar triage ─────────────────────────────────────────────────────────
//
// A mirror, variant for variant. Every decision behind these values was made in
// `bae_core::import::triage`.

#[cfg(feature = "desktop")]
impl crate::types::BridgeTriageQueue {
    pub(crate) fn from_core(queue: bae_core::import::TriageQueue) -> Self {
        let bae_core::import::TriageQueue {
            sections,
            counts,
            folder_scan_statuses,
        } = queue;
        let bae_core::import::TriageTabCounts {
            ready,
            needs_you,
            done,
            skipped,
        } = counts;
        crate::types::BridgeTriageQueue {
            sections: sections
                .into_iter()
                .map(crate::types::BridgeTriageSection::from_core)
                .collect(),
            counts: crate::types::BridgeTriageTabCounts {
                ready,
                needs_you,
                done,
                skipped,
            },
            folder_scan_statuses: folder_scan_statuses
                .into_iter()
                .map(crate::types::BridgeWatchedFolderScanStatus::from_core)
                .collect(),
        }
    }
}

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
            matched,
            selectable,
            import_status,
            picked,
            claim,
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
            matched: matched.map(crate::types::BridgeMatchedRelease::from_core),
            selectable,
            import_status: import_status.map(crate::types::BridgeCandidateImportStatus::from_core),
            picked: picked.map(crate::types::BridgeIdentityPick::from_core),
            claim: claim.map(crate::types::BridgeIdentityChoice::from_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeTriageSection {
    pub(super) fn from_core(section: bae_core::import::TriageSection) -> Self {
        Self {
            tab: crate::types::BridgeTriageTab::from_core(section.tab),
            watched_folder_path: section.watched_folder_path,
            group: section.group.map(|group| crate::types::BridgeTriageGroup {
                key: crate::types::BridgeFolderReleaseDecisionKey::from_core(group.key),
                name: group.name,
            }),
            entries: section
                .entries
                .into_iter()
                .map(|entry| {
                    let stable_key = entry.stable_key();
                    match entry {
                        bae_core::import::TriageEntry::Candidate(row) => {
                            crate::types::BridgeTriageEntry::Candidate {
                                stable_key,
                                row: crate::types::BridgeTriageRow::from_core(row),
                            }
                        }
                        bae_core::import::TriageEntry::Boundary(boundary) => {
                            crate::types::BridgeTriageEntry::Boundary {
                                stable_key,
                                boundary: crate::types::BridgeFolderReleaseBoundary::from_core(
                                    boundary,
                                ),
                            }
                        }
                        bae_core::import::TriageEntry::Invalid(candidate) => {
                            crate::types::BridgeTriageEntry::Invalid {
                                stable_key,
                                invalid_candidate: crate::types::BridgeInvalidCandidate::from_core(
                                    candidate,
                                ),
                            }
                        }
                    }
                })
                .collect(),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeTriageTab {
    pub(super) fn from_core(tab: bae_core::import::TriageTab) -> Self {
        match tab {
            bae_core::import::TriageTab::Ready => Self::Ready,
            bae_core::import::TriageTab::NeedsYou => Self::NeedsYou,
            bae_core::import::TriageTab::Done => Self::Done,
            bae_core::import::TriageTab::Skipped => Self::Skipped,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeTriagePlacement {
    pub(crate) fn from_core(placement: bae_core::import::TriagePlacement) -> Self {
        use bae_core::import::TriagePlacement as P;
        match placement {
            P::Ready => Self::Ready,
            P::NeedsYou { group, reason } => Self::NeedsYou {
                group: crate::types::BridgeNeedsYouGroup::from_core(group),
                reason: crate::types::BridgeNeedsYouReason::from_core(reason),
            },
            P::Importing => Self::Importing,
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
            G::SignalsDisagree => Self::SignalsDisagree,
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
            N::SignalsConflict => Self::SignalsConflict,
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

/// Every `UiBusEvent` has a bridge mirror, so this always returns `Some`.
pub(super) fn convert_ui_event(
    event: bae_core::ui::UiBusEvent,
) -> Option<crate::types::BridgeUiEvent> {
    use crate::types::*;
    use bae_core::ui::UiBusEvent;

    match event {
        UiBusEvent::PlaybackError { reason } => Some(BridgeUiEvent::PlaybackError {
            reason: crate::types::BridgePlaybackErrorReason::from_core(reason),
        }),
        UiBusEvent::QueueItemsAdded { count } => Some(BridgeUiEvent::QueueItemsAdded { count }),

        // ── Import live progress ───────────────────────────────────
        UiBusEvent::CandidateImportLoudnessProgress {
            key,
            tracks_done,
            tracks_total,
            fraction,
        } => Some(BridgeUiEvent::CandidateImportLoudnessProgress {
            key,
            tracks_done,
            tracks_total,
            fraction,
        }),
        UiBusEvent::ImportQueueIdentifyProgress { identified, total } => {
            Some(BridgeUiEvent::ImportQueueIdentifyProgress { identified, total })
        }

        // ── Errors ─────────────────────────────────────────────────
        UiBusEvent::Error { error } => Some(BridgeUiEvent::Error {
            error: crate::types::BridgeError::from_core(error),
        }),
    }
}
