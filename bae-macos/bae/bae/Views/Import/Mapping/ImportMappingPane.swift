import BaeKit
import SwiftUI

/// The one surface a folder becomes a release on: the folder it is about, two
/// sections and a commit bar. Section one is where the folder's metadata comes
/// from; section two is every source unit it offers alongside the track
/// committing makes of it.
///
/// There is no source⇄confirm layout flip. The table is the same table before
/// and after a metadata source is selected — selecting one fills its BECOMES
/// column in place.
struct ImportMappingPane: View {
    let candidate: Candidate
    /// What is in flight for this candidate: the run whose state the pane
    /// shows while one is live, and how far a running import has got. `nil`
    /// when nothing is running for it.
    let runtime: BridgeCandidateRuntimeSnapshot?
    /// What each track sheet may be bound to, by the sheet's file id.
    let bindingOptions: [String: [BridgeSheetBindingOption]]
    /// The path currently auditioning, if any.
    let previewingPath: String?
    let libraryStatus: BridgeLibraryStatus?
    let hasCoverOptions: Bool
    let coverContent: ImageContent?
    /// Where an album-level field's typed value goes: a row under this
    /// candidate, written as the field is left.
    let editActions: ReleaseFieldWriter
    @Binding
    var storageCloud: Bool
    @Binding
    var storagePinned: Bool
    let mappingActions: ImportMappingActions
    let commitActions: ImportCommitActions
    let onPresentMetadataMode: (BridgeImportMetadataMode) -> Void
    let onFindRelease: () -> Void
    let onReadFileTags: () -> Void
    /// Pick one of identification's matched pressings from the inline options
    /// — the same pick a search-sheet row click runs.
    let onPickRelease: (BridgeMetadataResult) -> Void
    let onUseFileTags: () -> Void
    let onEnterManually: () -> Void
    let onEditCover: () -> Void
    let onNavigateToPlacement: (String) -> Void

    /// The folder's mapping, as core reads it back for this candidate.
    private var mapping: BridgeMappingTable {
        candidate.mapping
    }

    /// The run in flight while there is one, else the state the stored verdict
    /// stands back up as.
    private var identifyState: IdentifyState {
        shownIdentifyState(
            resumed: candidate.resumedIdentifyState,
            runtime: runtime
        )
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                CandidateFolderLine(
                    placement: candidate.row?.placement,
                    folderName: candidate.displayName,
                    folderPath: candidate.key,
                    formatLabel: candidate.files.formatLabel,
                    onNavigateToPlacement: {
                        onNavigateToPlacement(candidate.key)
                    }
                )
                metadataSourceSection
                banners
                if candidate.detail != nil {
                    if !mapping.images.isEmpty {
                        ImportMappingGallery(
                            images: mapping.images,
                            evidence: candidate.fileEvidence,
                            actions: mappingActions
                        )
                    }
                    ImportMappingTable(
                        table: mapping,
                        bindingOptions: bindingOptions,
                        previewingPath: previewingPath,
                        unprobed: Set(candidate.detail?.unprobed ?? []),
                        evidence: candidate.fileEvidence,
                        actions: mappingActions,
                    )
                }
            }
            .padding(20)
        }
    }

    /// The card's commit row — present exactly when there is something to
    /// commit, which is the precondition the commit itself reads: a failed
    /// re-pick leaves the table and the album fields in place but nothing
    /// settled to commit them under.
    private var commitControls: ImportCommitControls? {
        guard candidate.presentedMetadataModeHasSelectedSeed else {
            return nil
        }
        return ImportCommitControls(
            unansweredCount: mapping.unansweredCount,
            candidateKey: candidate.key,
            importStatus: candidate.row?.importStatus,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned,
            actions: commitActions,
        )
    }

    private var metadataSourceSection: some View {
        ImportMetadataSourceSection(
            mode: candidate.presentedMetadataMode,
            releaseSummary: candidate.presentedMetadataModeHasSelectedSeed
                ? candidate.edit.map {
                    ImportReleaseSummary(candidate: candidate, editValues: $0)
                } : nil,
            fileTagsPreviewSummary: candidate.fileTagsPreview.edit.map {
                ImportReleaseSummary(candidate: candidate, fileTags: $0)
            },
            isReading: candidate.seedInFlight != nil
                || candidate.fileTagsPreview.isLoading,
            coverContent: coverContent,
            hasCoverOptions: hasCoverOptions,
            editValues: candidate.edit,
            editActions: editActions,
            matchOptions: matchOptions,
            hasSelectedSeed: candidate.presentedMetadataModeHasSelectedSeed,
            commit: commitControls,
            onPresentMode: onPresentMetadataMode,
            onFindRelease: onFindRelease,
            onReadFileTags: onReadFileTags,
            onUseFileTags: onUseFileTags,
            onEnterManually: onEnterManually,
            onEditCover: onEditCover,
        )
    }

    /// Identification's matches, offered inline while Lookup has no selected
    /// release. They stay up through the release read — the clicked row carries
    /// the spinner — and hand over to the release card after its detail lands.
    private var matchOptions: ImportMatchOptions? {
        guard candidate.presentedMetadataMode == .lookup,
            !candidate.presentedMetadataModeHasSelectedSeed,
            case .found(let groups, let libraryStatuses, _, let provenance) =
                identifyState
        else {
            return nil
        }
        return ImportMatchOptions(
            groups: groups,
            libraryStatuses: libraryStatuses,
            provenance: provenance,
            isImporting: ImportSearchFlow.isImporting(candidate),
            loadingReleaseId: candidate.loadingReleaseId,
            onSelect: onPickRelease,
        )
    }

    private var banners: some View {
        ImportConfirmationBanners(
            libraryStatus: libraryStatus,
            importStatus: candidate.row?.importStatus,
            error: candidate.error,
            failure: candidate.failure,
            onRetry: commitActions.confirmImport,
            onViewInLibrary: commitActions.viewInLibrary,
        )
    }
}
