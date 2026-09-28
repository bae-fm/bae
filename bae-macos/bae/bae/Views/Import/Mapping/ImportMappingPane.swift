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
    /// The exact source window currently auditioning, if any.
    let previewingTarget: BridgePreviewTarget?
    /// Whether a metadata source is being read into this candidate's draft.
    /// The store answers it: a pick outlives which candidate is on screen.
    let isApplyingMetadata: Bool
    let libraryStatus: BridgeLibraryStatus?
    let hasCoverOptions: Bool
    let coverContent: ImageContent?
    /// Where an album-level field's typed value goes: a row under this
    /// candidate, written as the field is left.
    let editActions: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands
    let endEditing: @MainActor () async -> Void
    @Binding
    var storageCloud: Bool
    @Binding
    var storagePinned: Bool
    let mappingActions: ImportMappingActions
    let commitActions: ImportCommitActions
    /// Move the pane as the person asked.
    let onMovePane: (BridgePaneMove) -> Void
    let onIdentify: () -> Void
    let onSearchForRelease: () -> Void
    let onReset: () -> Void
    let onResetToFileMetadata: () -> Void
    let onClearMetadata: () -> Void
    let onEditCover: () -> Void
    let onSelectCover: (BridgeCoverSelection) -> Void
    let onNavigateToPlacement: (String) -> Void

    /// The folder's mapping, as core reads it back for this candidate.
    private var mapping: BridgeMappingTable {
        candidate.mapping
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: ThemeSpace.section) {
                CandidateFolderLine(
                    tab: candidate.tab,
                    folderName: candidate.displayName,
                    folderPaths: candidate.sourceFolderPaths,
                    onNavigateToPlacement: {
                        onNavigateToPlacement(candidate.key)
                    }
                )
                metadataSourceSection
                banners
                if candidate.detail != nil {
                    if !mapping.images.isEmpty {
                        imagesSection
                    }
                    ImportMappingTable(
                        table: mapping,
                        previewingTarget: previewingTarget,
                        evidence: candidate.fileEvidence,
                        actions: mappingActions,
                        editingCommands: editingCommands,
                    )
                    .disabled(candidate.detail?.actionable != true)
                }
            }
            .padding(.horizontal, ThemeSpace.section)
            .padding(.top, ThemeSpace.section)
            .padding(.bottom, ThemeSpace.page)
        }
        .environment(
            \.artistResolutions,
            candidate.detail?.artistResolutions ?? []
        )
    }

    /// The folder's images under their own ruled heading, level with the
    /// Tracks heading the table draws beneath them.
    private var imagesSection: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            FormSectionHeader(title: String(localized: "Images"), ruled: true)
            ImportMappingGallery(
                images: mapping.images,
                evidence: candidate.fileEvidence,
                onOpen: mappingActions.openImages
            )
        }
    }

    /// The card's commit row, absent while the draft is entirely blank.
    private var commitControls: ImportCommitControls? {
        guard candidate.detail != nil, !candidate.metadataDraftIsBlank else {
            return nil
        }
        return ImportCommitControls(
            unansweredCount: mapping.unansweredCount,
            folderCheck: candidate.folderCheck,
            importStatus: candidate.importStatus,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned,
            actions: commitActions,
        )
    }

    private var metadataSourceSection: some View {
        ImportMetadataSourceSection(
            candidate: candidate,
            actionable: candidate.detail?.actionable == true,
            runtime: runtime,
            isReading: isApplyingMetadata,
            coverContent: coverContent,
            hasCoverOptions: hasCoverOptions,
            editActions: editActions,
            editingCommands: editingCommands,
            endEditing: endEditing,
            commit: commitControls,
            onMovePane: onMovePane,
            onIdentify: onIdentify,
            onSearchForRelease: onSearchForRelease,
            onReset: onReset,
            onResetToFileMetadata: onResetToFileMetadata,
            onClearMetadata: onClearMetadata,
            onEditCover: onEditCover,
            onSelectCover: onSelectCover,
        )
    }

    private var banners: some View {
        ImportConfirmationBanners(
            libraryStatus: libraryStatus,
            importStatus: candidate.importStatus,
            error: candidate.error,
            failure: candidate.failure,
            canEdit: candidate.detail?.actionable == true,
            onRetry: commitActions.confirmImport,
            onMergeArtists: commitActions.mergeArtists,
            onViewInLibrary: commitActions.viewInLibrary,
        )
    }
}
