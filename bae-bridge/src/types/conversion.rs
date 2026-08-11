use super::*;

#[cfg(feature = "desktop")]
impl BridgeMetadataResult {
    pub(crate) fn from_core(r: bae_core::import::search::MetadataResult) -> Self {
        let bae_core::import::search::MetadataResult {
            source,
            release_id,
            year,
            format,
            label,
            catalog_number,
            country,
            // Dropped: the card carries the album's title/artist/cover, so a
            // pressing projection keeps only pressing-distinguishing fields.
            title: _,
            artist: _,
            cover_art: _,
            source_group_id: _,
            // The source's own tracklist is Ready-rule evidence, not something
            // a pressing row renders; the sidebar reads the classification the
            // rule produced from it.
            source_tracks: _,
        } = r;
        BridgeMetadataResult {
            source: BridgeMetadataSource::from_core(source),
            release_id,
            year,
            format,
            label,
            catalog_number,
            country,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeRemoteCover {
    pub(crate) fn from_core(c: bae_core::import::cover_art::RemoteCover) -> Self {
        let bae_core::import::cover_art::RemoteCover {
            url,
            thumbnail_url,
            label,
            source,
        } = c;
        let selection = bridge_remote_cover_selection(url, source);
        let cover_choice = remote_cover_choice_to_bridge(&selection, &thumbnail_url);
        BridgeRemoteCover {
            cover_choice,
            label,
        }
    }
}

#[cfg(feature = "desktop")]
fn bridge_remote_cover_selection(
    url: String,
    source: bae_core::import::MetadataSource,
) -> BridgeRemoteCoverSelection {
    BridgeRemoteCoverSelection {
        url,
        source: BridgeMetadataSource::from_core(source),
    }
}

#[cfg(feature = "desktop")]
fn remote_cover_choice_to_bridge(
    selection: &BridgeRemoteCoverSelection,
    thumbnail_url: &str,
) -> BridgeCoverChoice {
    BridgeCoverChoice {
        selection: BridgeCoverSelection::RemoteCover {
            selection: selection.clone(),
        },
        preview_source: BridgeCoverImageSource::Remote {
            url: selection.url.clone(),
        },
        thumbnail_source: BridgeCoverImageSource::Remote {
            url: thumbnail_url.to_string(),
        },
    }
}

#[cfg(feature = "desktop")]
impl BridgeReleaseDetail {
    pub(crate) fn from_core(d: bae_core::import::search::ImportSearchReleaseDetail) -> Self {
        // Derived values borrow `&d`; compute them before destructuring `d`.
        let default_cover = d
            .default_cover()
            .cloned()
            .map(BridgeRemoteCover::from_core)
            .map(|c| c.cover_choice);
        let bae_core::import::search::ImportSearchReleaseDetail {
            release_id,
            source,
            source_group_id,
            title,
            artist,
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
            track_count,
            tracks,
            cover_art,
        } = d;
        BridgeReleaseDetail {
            release_id,
            source: BridgeMetadataSource::from_core(source),
            source_group_id,
            title,
            artist,
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
            track_count,
            tracks: tracks
                .into_iter()
                .map(BridgeReleaseTrack::from_core)
                .collect(),
            cover_art: cover_art
                .into_iter()
                .map(BridgeRemoteCover::from_core)
                .collect(),
            default_cover,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeReleaseTrack {
    pub(crate) fn from_core(t: bae_core::import::search::ReleaseTrack) -> Self {
        let bae_core::import::search::ReleaseTrack {
            title,
            artist,
            duration_ms,
            position,
            side,
        } = t;
        Self {
            title,
            artist,
            duration_ms,
            position,
            side,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeReleasePrefetch {
    pub(crate) fn from_core(p: bae_core::import::search::ImportReleasePrefetch) -> Self {
        let bae_core::import::search::ImportReleasePrefetch {
            detail,
            seed,
            claim,
            mapping,
        } = p;
        // The seed crosses masked for the claim the pick settled, so the editor
        // binds it directly. Doing it here rather than in the UI is what keeps
        // the two desktop surfaces from each deciding what an album-level claim
        // shows.
        let exact_pressing = BridgeRawPressingEdit::from_core(
            bae_core::import::RawPressingEdit::from_pressing(&seed.pressing),
        );
        let seed = bae_core::import::shape_user_edit_for_choice(&seed, &claim.choice);
        BridgeReleasePrefetch {
            detail: BridgeReleaseDetail::from_core(detail),
            seed: BridgeReleaseUserEdit::from_core(seed),
            claim: BridgeClaimLine::from_core(claim),
            exact_pressing,
            mapping: BridgeMappingTable::from_core(mapping),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeClaimLine {
    pub(crate) fn from_core(claim: bae_core::import::ClaimLine) -> Self {
        let bae_core::import::ClaimLine {
            choice,
            level,
            evidence,
            release,
            track_count,
        } = claim;
        BridgeClaimLine {
            choice: BridgeIdentityChoice::from_core(choice),
            level: BridgeClaimLevel::from_core(level),
            evidence: BridgeClaimEvidence::from_core(evidence),
            release,
            track_count,
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::ClaimLine {
        let BridgeClaimLine {
            choice,
            level,
            evidence,
            release,
            track_count,
        } = self;
        bae_core::import::ClaimLine {
            choice: choice.into_core(),
            level: level.into_core(),
            evidence: evidence.into_core(),
            release,
            track_count,
        }
    }
}

/// The claim an edited release still supports.
///
/// Holding exactly this pressing is a claim about the values on the screen, so
/// editing one of them away lowers the claim to the album. Nothing here raises
/// one: a claim is the user's own assertion, and the control that makes it
/// restores the release's values itself.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_claim_for_edit(
    claim: BridgeClaimLine,
    edited: BridgeRawPressingEdit,
    exact: BridgeRawPressingEdit,
) -> BridgeClaimLine {
    BridgeClaimLine::from_core(bae_core::import::claim_for_edit(
        claim.into_core(),
        &edited.into_core(),
        &exact.into_core(),
    ))
}

#[cfg(feature = "desktop")]
impl BridgeClaimEvidence {
    pub(super) fn from_core(evidence: bae_core::import::ClaimEvidence) -> Self {
        use bae_core::import::ClaimEvidence;
        match evidence {
            ClaimEvidence::DiscIdAlone => BridgeClaimEvidence::DiscIdAlone,
            ClaimEvidence::DiscIdShared { match_count } => {
                BridgeClaimEvidence::DiscIdShared { match_count }
            }
            ClaimEvidence::Barcode => BridgeClaimEvidence::Barcode,
            ClaimEvidence::Search => BridgeClaimEvidence::Search,
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::ClaimEvidence {
        use bae_core::import::ClaimEvidence;
        match self {
            Self::DiscIdAlone => ClaimEvidence::DiscIdAlone,
            Self::DiscIdShared { match_count } => ClaimEvidence::DiscIdShared { match_count },
            Self::Barcode => ClaimEvidence::Barcode,
            Self::Search => ClaimEvidence::Search,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeDiscidProgress {
    pub(super) fn from_view(p: bae_core::identify::DiscidProgressView) -> Self {
        use bae_core::identify::DiscidProgressView;
        match p {
            DiscidProgressView::Computing => BridgeDiscidProgress::Computing,
            DiscidProgressView::LookingUp => BridgeDiscidProgress::LookingUp,
            DiscidProgressView::Done { n_results } => BridgeDiscidProgress::Done { n_results },
            DiscidProgressView::Skipped => BridgeDiscidProgress::Skipped,
            DiscidProgressView::Failed { failure } => BridgeDiscidProgress::Failed {
                failure: BridgeLookupFailure::from_core(failure),
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeBarcodeProgress {
    pub(super) fn from_view(p: bae_core::identify::BarcodeProgressView) -> Self {
        use bae_core::identify::BarcodeProgressView;
        match p {
            BarcodeProgressView::Scanning => BridgeBarcodeProgress::Scanning,
            BarcodeProgressView::LookingUp {
                current,
                position,
                total,
            } => BridgeBarcodeProgress::LookingUp {
                current,
                position,
                total,
            },
            BarcodeProgressView::Done { n_results } => BridgeBarcodeProgress::Done { n_results },
            BarcodeProgressView::Failed { failure } => BridgeBarcodeProgress::Failed {
                failure: BridgeLookupFailure::from_core(failure),
            },
            BarcodeProgressView::Skipped => BridgeBarcodeProgress::Skipped,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeReleaseGroup {
    pub(crate) fn from_core(g: bae_core::import::release_group::ReleaseGroup) -> Self {
        let bae_core::import::release_group::ReleaseGroup {
            id,
            title,
            artist,
            cover_art,
            source_label,
            group_url,
            year_min,
            year_max,
            pressings,
        } = g;
        BridgeReleaseGroup {
            id,
            title,
            artist,
            cover_art: cover_art.map(BridgeRemoteCover::from_core),
            source_label,
            group_url,
            year_min,
            year_max,
            pressings: pressings
                .into_iter()
                .map(BridgeMetadataResult::from_core)
                .collect(),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSignals {
    pub(crate) fn from_core(s: bae_core::signals::Signals) -> Self {
        use bae_core::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};

        fn sourced_values(values: Vec<bae_core::signals::SourcedValue>) -> Vec<BridgeSourcedValue> {
            values
                .into_iter()
                .map(BridgeSourcedValue::from_core)
                .collect()
        }

        let Signals {
            disc_id,
            barcode,
            text,
            // The probed total is a Ready-rule input, not a badge: the sidebar
            // reads a candidate's classification, and the mapping pane will
            // read per-file durations it probes for the one open candidate.
            // Neither wants this number, so it does not cross.
            probed_total_duration_ms: _,
        } = s;

        let disc_id = match disc_id {
            DiscIdSignal::Computed {
                disc_id,
                track_count,
            } => BridgeDiscIdSignal::Computed {
                disc_id,
                track_count,
            },
            DiscIdSignal::Absent { track_count } => BridgeDiscIdSignal::Absent { track_count },
            DiscIdSignal::Failed {
                failure,
                track_count,
            } => BridgeDiscIdSignal::Failed {
                failure: BridgeLookupFailure::from_core(failure),
                track_count,
            },
        };

        let barcode = match barcode {
            BarcodeSignal::Scanning { codes } => BridgeBarcodeSignal::Scanning {
                codes: sourced_values(codes),
            },
            BarcodeSignal::Settled { codes } => BridgeBarcodeSignal::Settled {
                codes: sourced_values(codes),
            },
            BarcodeSignal::Failed { failure, codes } => BridgeBarcodeSignal::Failed {
                failure: BridgeLookupFailure::from_core(failure),
                codes: sourced_values(codes),
            },
            BarcodeSignal::Absent => BridgeBarcodeSignal::Absent,
        };

        let text = match text {
            TextSignal::Scanning {
                catalogs,
                free_text,
            } => BridgeTextSignal::Scanning {
                catalogs: sourced_values(catalogs),
                free_text,
            },
            TextSignal::Settled {
                catalogs,
                free_text,
            } => BridgeTextSignal::Settled {
                catalogs: sourced_values(catalogs),
                free_text,
            },
            TextSignal::Failed {
                failure,
                catalogs,
                free_text,
            } => BridgeTextSignal::Failed {
                failure: BridgeLookupFailure::from_core(failure),
                catalogs: sourced_values(catalogs),
                free_text,
            },
        };

        BridgeSignals {
            disc_id,
            barcode,
            text,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeResultProvenance {
    pub(super) fn from_core(p: bae_core::identify::ResultProvenance) -> Self {
        let bae_core::identify::ResultProvenance {
            by_disc_id,
            by_barcode,
            matches_catalog,
        } = p;
        BridgeResultProvenance {
            by_disc_id,
            by_barcode,
            matches_catalog,
        }
    }
}

/// Mirror [`bae_core::identify::IdentifyStateView`] into the uniffi enum. Core has
/// already folded the matches into their group, keyed the provenance, reduced the
/// in-flight payloads to counts, and dropped what must not cross — this is a field
/// copy per variant and nothing else.
#[cfg(feature = "desktop")]
impl BridgeIdentifyState {
    pub(crate) fn from_core(s: bae_core::identify::IdentifyState) -> Self {
        use bae_core::identify::IdentifyStateView;
        match IdentifyStateView::from(s) {
            IdentifyStateView::Idle => BridgeIdentifyState::Idle,
            IdentifyStateView::Triangulating { discid, barcode } => {
                BridgeIdentifyState::Triangulating {
                    discid: BridgeDiscidProgress::from_view(discid),
                    barcode: BridgeBarcodeProgress::from_view(barcode),
                }
            }
            IdentifyStateView::Found {
                group,
                library_statuses,
                track_count,
                provenance,
            } => BridgeIdentifyState::Found {
                group: BridgeReleaseGroup::from_core(group),
                library_statuses: status_map(library_statuses),
                track_count,
                provenance: provenance
                    .into_iter()
                    .map(|(release_id, p)| (release_id, BridgeResultProvenance::from_core(p)))
                    .collect(),
            },
            IdentifyStateView::Conflict {
                discid_results,
                barcode_results,
                matched_barcode,
                track_count,
            } => {
                let (discid_results, discid_library_statuses) =
                    results_and_status_map(discid_results);
                let (barcode_results, barcode_library_statuses) =
                    results_and_status_map(barcode_results);
                BridgeIdentifyState::Conflict {
                    discid_results,
                    discid_library_statuses,
                    barcode_results,
                    barcode_library_statuses,
                    matched_barcode,
                    track_count,
                }
            }
            IdentifyStateView::NotFoundAnywhere => BridgeIdentifyState::NotFoundAnywhere,
            IdentifyStateView::ManualOnly { track_count } => {
                BridgeIdentifyState::ManualOnly { track_count }
            }
        }
    }
}

/// Key library statuses by release id — the UI looks a row's status up by id
/// rather than re-indexing a flat list. Each status carries its own id, so this
/// is a re-container, not a re-pairing.
#[cfg(feature = "desktop")]
fn status_map(
    statuses: Vec<bae_core::db::LibraryStatus>,
) -> std::collections::HashMap<String, BridgeLibraryStatus> {
    statuses
        .into_iter()
        .map(|s| (s.release_id.clone(), BridgeLibraryStatus::from_core(s)))
        .collect()
}

/// Unzip core's paired rows into the two containers the UI reads: the ordered
/// results list (display order matters) and their statuses keyed by release id.
#[cfg(feature = "desktop")]
fn results_and_status_map(
    rows: Vec<bae_core::identify::ResultRow>,
) -> (
    Vec<BridgeMetadataResult>,
    std::collections::HashMap<String, BridgeLibraryStatus>,
) {
    let (results, statuses): (Vec<_>, Vec<_>) = rows
        .into_iter()
        .map(|bae_core::identify::ResultRow { result, status }| {
            (BridgeMetadataResult::from_core(result), status)
        })
        .unzip();
    (results, status_map(statuses))
}

#[cfg(feature = "desktop")]
impl BridgeFileInfo {
    pub(super) fn from_core(f: bae_core::import::folder_scanner::ScannedFile) -> Self {
        let bae_core::import::folder_scanner::ScannedFile {
            path,
            relative_path,
            size,
            dir_prefix,
            file_name,
        } = f;
        BridgeFileInfo {
            name: relative_path,
            size,
            dir_prefix,
            file_name,
            local_path: path.to_string_lossy().to_string(),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeCandidateFile {
    pub(super) fn from_core(
        entry: bae_core::import::folder_scanner::CandidateFile,
        becomes: bae_core::import::folder_scanner::FileBecomes,
    ) -> Self {
        use bae_core::import::folder_scanner::{CandidateFile, FileRole, SheetBinding};

        let alternatives = entry
            .role_alternatives()
            .iter()
            .copied()
            .map(BridgeFileRoleChoice::from_core)
            .collect();
        let role_choice = entry.role_choice().map(BridgeFileRoleChoice::from_core);
        let CandidateFile {
            file,
            role,
            proposed_audio: _,
        } = entry;
        // Read the file id (relative path) and disk path back off `BridgeFileInfo`
        // so the exhaustive `ScannedFile` destructure lives only in its `from_core`.
        let file = BridgeFileInfo::from_core(file);
        let image_choice = || BridgeCoverChoice {
            selection: BridgeCoverSelection::ReleaseImage {
                file_id: file.name.clone(),
            },
            preview_source: BridgeCoverImageSource::Local {
                path: file.local_path.clone(),
            },
            thumbnail_source: BridgeCoverImageSource::Local {
                path: file.local_path.clone(),
            },
        };
        let role = match role {
            FileRole::Audio => BridgeFileRole::Audio,
            // The disc assignment is the mapping table's to show, on the group
            // header that carries the picker for it. A roles row states what
            // the sheet's slots are, which already reflects the assignment.
            FileRole::TrackSheet {
                sheet,
                binding,
                disc: _,
            } => BridgeFileRole::TrackSheet {
                binding: match binding {
                    SheetBinding::Describes { file_id } => {
                        BridgeSheetBinding::Describes { file_id }
                    }
                    // Derived from the parsed sheet, like `track_count` below:
                    // the directive's text is what the pane shows a user whose
                    // sheet found nothing, and the bridge doesn't mirror the
                    // whole parse to carry it.
                    SheetBinding::Unresolved => BridgeSheetBinding::Unresolved {
                        requested: sheet
                            .audio_file_references()
                            .into_iter()
                            .map(str::to_string)
                            .collect(),
                    },
                    SheetBinding::RefusedCodec { file_id, codec } => {
                        BridgeSheetBinding::RefusedCodec { file_id, codec }
                    }
                },
                // A derived count, not a carried field — `CueSheet` is a large
                // parse product the bridge doesn't mirror.
                track_count: sheet.playable_track_count() as u32,
            },
            FileRole::Cover => BridgeFileRole::Cover {
                choice: image_choice(),
            },
            FileRole::Artwork => BridgeFileRole::Artwork {
                choice: image_choice(),
            },
            FileRole::Document => BridgeFileRole::Document,
            FileRole::Other => BridgeFileRole::Other,
        };
        BridgeCandidateFile {
            file,
            role,
            becomes: BridgeFileBecomes::from_core(becomes),
            alternatives,
            role_choice,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeFileRoleChoice {
    pub(crate) fn from_core(choice: bae_core::import::folder_scanner::FileRoleChoice) -> Self {
        use bae_core::import::folder_scanner::FileRoleChoice;
        match choice {
            FileRoleChoice::Audio => Self::Audio,
            FileRoleChoice::NotATrack => Self::NotATrack,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::folder_scanner::FileRoleChoice {
        use bae_core::import::folder_scanner::FileRoleChoice;
        match self {
            Self::Audio => FileRoleChoice::Audio,
            Self::NotATrack => FileRoleChoice::NotATrack,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeFileBecomes {
    pub(super) fn from_core(becomes: bae_core::import::folder_scanner::FileBecomes) -> Self {
        use bae_core::import::folder_scanner::FileBecomes;
        match becomes {
            FileBecomes::Slots { first, last } => Self::Slots { first, last },
            FileBecomes::NoSlots => Self::NoSlots,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeCollapsedDirectory {
    pub(super) fn from_core(
        directory: bae_core::import::folder_scanner::CollapsedDirectory,
    ) -> Self {
        use bae_core::import::folder_scanner::{CollapsedDirectory, FileRowKind};
        let CollapsedDirectory {
            dir_prefix,
            kind,
            count,
            total_size,
        } = directory;
        BridgeCollapsedDirectory {
            dir_prefix,
            kind: match kind {
                FileRowKind::Document => BridgeFileRowKind::Document,
                FileRowKind::Other => BridgeFileRowKind::Other,
            },
            count,
            total_size,
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::folder_scanner::CollapsedDirectory {
        use bae_core::import::folder_scanner::{CollapsedDirectory, FileRowKind};
        let BridgeCollapsedDirectory {
            dir_prefix,
            kind,
            count,
            total_size,
        } = self;
        CollapsedDirectory {
            dir_prefix,
            kind: match kind {
                BridgeFileRowKind::Document => FileRowKind::Document,
                BridgeFileRowKind::Other => FileRowKind::Other,
            },
            count,
            total_size,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSheetBindingOption {
    pub(crate) fn from_core(option: bae_core::import::folder_scanner::SheetBindingOption) -> Self {
        use bae_core::import::folder_scanner::{SheetBindingOffer, SheetBindingOption};

        let SheetBindingOption { file_id, offer } = option;
        BridgeSheetBindingOption {
            file_id,
            offer: match offer {
                SheetBindingOffer::Offered => BridgeSheetBindingOffer::Offered,
                SheetBindingOffer::RefusedCodec { codec } => {
                    BridgeSheetBindingOffer::RefusedCodec { codec }
                }
                SheetBindingOffer::RefusedUnreadable => BridgeSheetBindingOffer::RefusedUnreadable,
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeCandidateFiles {
    pub(crate) fn from_core(files: bae_core::import::folder_scanner::CategorizedFiles) -> Self {
        // Both derived from the whole set before it is taken apart: which slots
        // a file backs and which directories collapse are facts about the
        // folder, not about any one file.
        let becomes = files.becomes();
        let collapsed_directories = files
            .collapsed_directories()
            .into_iter()
            .map(BridgeCollapsedDirectory::from_core)
            .collect();
        let bae_core::import::folder_scanner::CategorizedFiles {
            files,
            format_label,
        } = files;
        BridgeCandidateFiles {
            files: files
                .into_iter()
                .zip(becomes)
                .map(|(entry, becomes)| BridgeCandidateFile::from_core(entry, becomes))
                .collect(),
            format_label,
            collapsed_directories,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgePressingEdit {
    pub(super) fn from_core(p: bae_core::import::PressingEdit) -> Self {
        let bae_core::import::PressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        } = p;
        Self {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::PressingEdit {
        let BridgePressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        } = self;
        bae_core::import::PressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeAudioFile {
    pub(super) fn from_core(file: bae_core::import::AudioFile) -> Self {
        match file {
            bae_core::import::AudioFile::Standalone { file_id } => Self::Standalone { file_id },
            bae_core::import::AudioFile::SheetSlice {
                file_id,
                sheet_id,
                index,
            } => Self::SheetSlice {
                file_id,
                sheet_id,
                index,
            },
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::AudioFile {
        match self {
            Self::Standalone { file_id } => bae_core::import::AudioFile::Standalone { file_id },
            Self::SheetSlice {
                file_id,
                sheet_id,
                index,
            } => bae_core::import::AudioFile::SheetSlice {
                file_id,
                sheet_id,
                index,
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSlotReconciliation {
    pub(super) fn from_core(reconciliation: bae_core::import::SlotReconciliation) -> Self {
        use bae_core::import::SlotReconciliation;
        match reconciliation {
            SlotReconciliation::Agrees { count } => Self::Agrees { count },
            SlotReconciliation::MoreFiles { files, tracks } => Self::MoreFiles { files, tracks },
            SlotReconciliation::MoreTracks { files, tracks } => Self::MoreTracks { files, tracks },
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::SlotReconciliation {
        use bae_core::import::SlotReconciliation;
        match self {
            Self::Agrees { count } => SlotReconciliation::Agrees { count },
            Self::MoreFiles { files, tracks } => SlotReconciliation::MoreFiles { files, tracks },
            Self::MoreTracks { files, tracks } => SlotReconciliation::MoreTracks { files, tracks },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSheetDisc {
    pub(super) fn from_core(disc: bae_core::import::folder_scanner::SheetDisc) -> Self {
        use bae_core::import::folder_scanner::SheetDisc;
        match disc {
            SheetDisc::Disc { number } => Self::Disc { number },
            SheetDisc::Ignored => Self::Ignored,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::folder_scanner::SheetDisc {
        use bae_core::import::folder_scanner::SheetDisc;
        match self {
            Self::Disc { number } => SheetDisc::Disc { number },
            Self::Ignored => SheetDisc::Ignored,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingRole {
    pub(super) fn from_core(role: bae_core::import::MappingRole) -> Self {
        use bae_core::import::MappingRole;
        match role {
            MappingRole::Audio => Self::Audio,
            MappingRole::Document => Self::Document,
            MappingRole::Other => Self::Other,
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::MappingRole {
        use bae_core::import::MappingRole;
        match self {
            Self::Audio => MappingRole::Audio,
            Self::Document => MappingRole::Document,
            Self::Other => MappingRole::Other,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingFile {
    pub(super) fn from_core(file: bae_core::import::MappingFile) -> Self {
        let bae_core::import::MappingFile {
            file_id,
            name,
            size,
            path,
            probed_duration_ms,
            role,
            alternatives,
            role_choice,
        } = file;
        BridgeMappingFile {
            role: BridgeMappingRole::from_core(role),
            local_path: path.to_string_lossy().to_string(),
            file_id,
            name,
            size,
            probed_duration_ms,
            alternatives: alternatives
                .into_iter()
                .map(BridgeFileRoleChoice::from_core)
                .collect(),
            role_choice: role_choice.map(BridgeFileRoleChoice::from_core),
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::MappingFile {
        let BridgeMappingFile {
            file_id,
            name,
            size,
            local_path,
            probed_duration_ms,
            role,
            alternatives,
            role_choice,
        } = self;
        bae_core::import::MappingFile {
            file_id,
            name,
            size,
            path: std::path::PathBuf::from(local_path),
            probed_duration_ms,
            role: role.into_core(),
            alternatives: alternatives
                .into_iter()
                .map(BridgeFileRoleChoice::into_core)
                .collect(),
            role_choice: role_choice.map(BridgeFileRoleChoice::into_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingEntry {
    pub(super) fn from_core(entry: bae_core::import::MappingEntry) -> Self {
        let bae_core::import::MappingEntry {
            sheet_id,
            index,
            number,
            title,
            duration_ms,
            container_id,
            container_name,
            container_path,
        } = entry;
        BridgeMappingEntry {
            sheet_id,
            index,
            number,
            title,
            duration_ms,
            container_id,
            container_name,
            container_local_path: container_path.to_string_lossy().to_string(),
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::MappingEntry {
        let BridgeMappingEntry {
            sheet_id,
            index,
            number,
            title,
            duration_ms,
            container_id,
            container_name,
            container_local_path,
        } = self;
        bae_core::import::MappingEntry {
            sheet_id,
            index,
            number,
            title,
            duration_ms,
            container_id,
            container_name,
            container_path: std::path::PathBuf::from(container_local_path),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingSource {
    pub(super) fn from_core(source: bae_core::import::MappingSource) -> Self {
        use bae_core::import::MappingSource;
        match source {
            MappingSource::File(file) => Self::File {
                file: BridgeMappingFile::from_core(file),
            },
            MappingSource::SheetEntry(entry) => Self::SheetEntry {
                entry: BridgeMappingEntry::from_core(entry),
            },
            MappingSource::Missing => Self::Missing,
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::MappingSource {
        use bae_core::import::MappingSource;
        match self {
            Self::File { file } => MappingSource::File(file.into_core()),
            Self::SheetEntry { entry } => MappingSource::SheetEntry(entry.into_core()),
            Self::Missing => MappingSource::Missing,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingBecomes {
    pub(super) fn from_core(becomes: bae_core::import::MappingBecomes) -> Self {
        use bae_core::import::MappingBecomes;
        match becomes {
            MappingBecomes::Track {
                track,
                source_position,
                source_duration_ms,
            } => Self::Track {
                track: BridgeRawTrackEdit::from_core(track),
                source_position,
                source_duration_ms,
            },
            MappingBecomes::Kept => Self::Kept,
            MappingBecomes::AwaitingPick => Self::AwaitingPick,
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::MappingBecomes {
        use bae_core::import::MappingBecomes;
        match self {
            Self::Track {
                track,
                source_position,
                source_duration_ms,
            } => MappingBecomes::Track {
                track: track.into_core(),
                source_position,
                source_duration_ms,
            },
            Self::Kept => MappingBecomes::Kept,
            Self::AwaitingPick => MappingBecomes::AwaitingPick,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingUnit {
    pub(super) fn from_core(unit: bae_core::import::MappingUnit) -> Self {
        let bae_core::import::MappingUnit { source, becomes } = unit;
        BridgeMappingUnit {
            source: BridgeMappingSource::from_core(source),
            becomes: BridgeMappingBecomes::from_core(becomes),
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::MappingUnit {
        let BridgeMappingUnit { source, becomes } = self;
        bae_core::import::MappingUnit {
            source: source.into_core(),
            becomes: becomes.into_core(),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingContainer {
    pub(super) fn from_core(container: bae_core::import::MappingContainer) -> Self {
        let bae_core::import::MappingContainer {
            file_id,
            name,
            size,
        } = container;
        BridgeMappingContainer {
            file_id,
            name,
            size,
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::MappingContainer {
        let BridgeMappingContainer {
            file_id,
            name,
            size,
        } = self;
        bae_core::import::MappingContainer {
            file_id,
            name,
            size,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSheetGroup {
    pub(super) fn from_core(sheet: bae_core::import::SheetGroup) -> Self {
        let bae_core::import::SheetGroup {
            sheet_id,
            name,
            path,
            bound,
            assignment,
            disc_options,
        } = sheet;
        BridgeSheetGroup {
            sheet_id,
            name,
            local_path: path.to_string_lossy().into_owned(),
            bound: BridgeSheetBound::from_core(bound),
            assignment: BridgeSheetDisc::from_core(assignment),
            disc_options,
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::SheetGroup {
        let BridgeSheetGroup {
            sheet_id,
            name,
            local_path,
            bound,
            assignment,
            disc_options,
        } = self;
        bae_core::import::SheetGroup {
            sheet_id,
            name,
            path: std::path::PathBuf::from(local_path),
            bound: bound.into_core(),
            assignment: assignment.into_core(),
            disc_options,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSheetBound {
    pub(super) fn from_core(bound: bae_core::import::SheetBound) -> Self {
        use bae_core::import::SheetBound;
        match bound {
            SheetBound::Describes(container) => Self::Describes {
                container: BridgeMappingContainer::from_core(container),
            },
            SheetBound::Unresolved { requested } => Self::Unresolved { requested },
            SheetBound::RefusedCodec { container, codec } => Self::RefusedCodec {
                container: BridgeMappingContainer::from_core(container),
                codec,
            },
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::SheetBound {
        use bae_core::import::SheetBound;
        match self {
            Self::Describes { container } => SheetBound::Describes(container.into_core()),
            Self::Unresolved { requested } => SheetBound::Unresolved { requested },
            Self::RefusedCodec { container, codec } => SheetBound::RefusedCodec {
                container: container.into_core(),
                codec,
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingImage {
    pub(super) fn from_core(image: bae_core::import::MappingImage) -> Self {
        let bae_core::import::MappingImage {
            file_id,
            name,
            size,
            path,
            is_cover,
        } = image;
        BridgeMappingImage {
            file_id,
            name,
            size,
            local_path: path.to_string_lossy().to_string(),
            is_cover,
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::MappingImage {
        let BridgeMappingImage {
            file_id,
            name,
            size,
            local_path,
            is_cover,
        } = self;
        bae_core::import::MappingImage {
            file_id,
            name,
            size,
            path: std::path::PathBuf::from(local_path),
            is_cover,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingRow {
    pub(super) fn from_core(row: bae_core::import::MappingRow) -> Self {
        use bae_core::import::MappingRow;
        match row {
            MappingRow::Unit(unit) => Self::Unit {
                unit: BridgeMappingUnit::from_core(unit),
            },
            MappingRow::Sheet { sheet, entries } => Self::Sheet {
                sheet: BridgeSheetGroup::from_core(sheet),
                entries: entries
                    .into_iter()
                    .map(BridgeMappingUnit::from_core)
                    .collect(),
            },
            MappingRow::Images(images) => Self::Images {
                images: images
                    .into_iter()
                    .map(BridgeMappingImage::from_core)
                    .collect(),
            },
            MappingRow::Directory(directory) => Self::Directory {
                directory: BridgeCollapsedDirectory::from_core(directory),
            },
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::MappingRow {
        use bae_core::import::MappingRow;
        match self {
            Self::Unit { unit } => MappingRow::Unit(unit.into_core()),
            Self::Sheet { sheet, entries } => MappingRow::Sheet {
                sheet: sheet.into_core(),
                entries: entries
                    .into_iter()
                    .map(BridgeMappingUnit::into_core)
                    .collect(),
            },
            Self::Images { images } => MappingRow::Images(
                images
                    .into_iter()
                    .map(BridgeMappingImage::into_core)
                    .collect(),
            ),
            Self::Directory { directory } => MappingRow::Directory(directory.into_core()),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingTable {
    pub(crate) fn from_core(table: bae_core::import::MappingTable) -> Self {
        let bae_core::import::MappingTable {
            rows,
            reconciliation,
        } = table;
        BridgeMappingTable {
            rows: rows.into_iter().map(BridgeMappingRow::from_core).collect(),
            reconciliation: reconciliation.map(BridgeSlotReconciliation::from_core),
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::MappingTable {
        let BridgeMappingTable {
            rows,
            reconciliation,
        } = self;
        bae_core::import::MappingTable {
            rows: rows.into_iter().map(BridgeMappingRow::into_core).collect(),
            reconciliation: reconciliation.map(BridgeSlotReconciliation::into_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeTrackUserEdit {
    pub(super) fn from_core(t: bae_core::import::TrackUserEdit) -> Self {
        let bae_core::import::TrackUserEdit {
            title,
            side,
            track_number,
            artist_names,
            file,
        } = t;
        Self {
            title,
            side,
            track_number,
            artist_names,
            file: file.map(BridgeAudioFile::from_core),
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::TrackUserEdit {
        let BridgeTrackUserEdit {
            title,
            side,
            track_number,
            artist_names,
            file,
        } = self;
        bae_core::import::TrackUserEdit {
            title,
            side,
            track_number,
            artist_names,
            file: file.map(BridgeAudioFile::into_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeReleaseUserEdit {
    pub(crate) fn from_core(e: bae_core::import::ReleaseUserEdit) -> Self {
        let bae_core::import::ReleaseUserEdit {
            album_title,
            album_artist_names,
            pressing,
            tracks,
        } = e;
        BridgeReleaseUserEdit {
            album_title,
            album_artist_names,
            pressing: BridgePressingEdit::from_core(pressing),
            tracks: tracks
                .into_iter()
                .map(BridgeTrackUserEdit::from_core)
                .collect(),
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::ReleaseUserEdit {
        let BridgeReleaseUserEdit {
            album_title,
            album_artist_names,
            pressing,
            tracks,
        } = self;
        bae_core::import::ReleaseUserEdit {
            album_title,
            album_artist_names,
            pressing: pressing.into_core(),
            tracks: tracks
                .into_iter()
                .map(BridgeTrackUserEdit::into_core)
                .collect(),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeRawPressingEdit {
    pub(super) fn from_core(p: bae_core::import::RawPressingEdit) -> Self {
        let bae_core::import::RawPressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        } = p;
        Self {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::RawPressingEdit {
        let BridgeRawPressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        } = self;
        bae_core::import::RawPressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeRawTrackEdit {
    pub(super) fn from_core(t: bae_core::import::RawTrackEdit) -> Self {
        let bae_core::import::RawTrackEdit {
            id,
            title,
            artist_text,
            side,
            track_number,
            file,
        } = t;
        Self {
            id,
            title,
            artist_text,
            side,
            track_number,
            file: file.map(BridgeAudioFile::from_core),
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::RawTrackEdit {
        let BridgeRawTrackEdit {
            id,
            title,
            artist_text,
            side,
            track_number,
            file,
        } = self;
        bae_core::import::RawTrackEdit {
            id,
            title,
            artist_text,
            side,
            track_number,
            file: file.map(BridgeAudioFile::into_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeRawReleaseEdit {
    pub(crate) fn from_core(e: bae_core::import::RawReleaseEdit) -> Self {
        let bae_core::import::RawReleaseEdit {
            album_title,
            album_artist_text,
            pressing,
            tracks,
        } = e;
        BridgeRawReleaseEdit {
            album_title,
            album_artist_text,
            pressing: BridgeRawPressingEdit::from_core(pressing),
            tracks: tracks
                .into_iter()
                .map(BridgeRawTrackEdit::from_core)
                .collect(),
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::RawReleaseEdit {
        let BridgeRawReleaseEdit {
            album_title,
            album_artist_text,
            pressing,
            tracks,
        } = self;
        bae_core::import::RawReleaseEdit {
            album_title,
            album_artist_text,
            pressing: pressing.into_core(),
            tracks: tracks
                .into_iter()
                .map(BridgeRawTrackEdit::into_core)
                .collect(),
        }
    }
}
