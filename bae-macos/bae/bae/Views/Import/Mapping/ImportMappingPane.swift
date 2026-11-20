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
    var importDestination: BridgeImportDestination
    let mappingActions: ImportMappingActions
    let commitActions: ImportCommitActions
    let albumActions: DraftAlbumActions
    /// Move the pane as the person asked.
    let onMovePane: (PaneMove) -> Void
    let onIdentify: () -> Void
    let onSearchForRelease: () -> Void
    let onResetToFileMetadata: () -> Void
    let onClearMetadata: () -> Void
    let onUnlink: () -> Void
    let onEditCover: () -> Void
    let onDropCoverImage: (String) -> Void
    let onNavigateToPlacement: (Data) -> Void

    /// The folder's mapping, as core reads it back for this candidate.
    private var mapping: BridgeMappingTable {
        candidate.mapping
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: ThemeSpace.group) {
                MacImportFolderLine(
                    tab: candidate.tab,
                    folderName: candidate.displayName,
                    folderPaths: candidate.sourceFolderPaths,
                    onNavigateToPlacement: {
                        onNavigateToPlacement(candidate.key)
                    }
                )
                banners
                editor
            }
            .padding(.horizontal, ThemeSpace.section)
            .padding(.top, ThemeSpace.section)
            .padding(.bottom, ThemeSpace.page)
        }
    }

    private var actionable: Bool {
        candidate.detail?.actionable == true
    }

    private var searching: Bool {
        candidate.metadataPresentation == .findOnline
    }

    /// The release editor over the candidate's draft, or Find online while
    /// the draft is still a read away.
    @ViewBuilder
    private var editor: some View {
        if let edit = candidate.edit,
            let offers = candidate.draft?.pressingOffers
        {
            MacReleaseEditor(
                actionRow: .importing(commitControls),
                commands: ReleaseEditorCommands(
                    identifyAutomatically: onIdentify,
                    search: onSearchForRelease,
                    metadataCommand: { command in
                        command.run(
                            resetToFileMetadata: onResetToFileMetadata,
                            clearMetadata: onClearMetadata
                        )
                    },
                    unlink: onUnlink,
                    identifyDisabled: isApplyingMetadata
                ),
                card: ReleaseEditorCard(
                    values: { edit },
                    pressingOffers: offers,
                    albumArtists: candidate.draft?.albumArtists,
                    writer: editActions,
                    cover: ReleaseCover(
                        content: coverContent,
                        onEdit: hasCoverOptions ? onEditCover : nil,
                        onDropImage: onDropCoverImage
                    ),
                    sourceAudio: candidate.files.sourceAudio,
                    // A blank draft files under nothing yet.
                    album: candidate.metadataDraftIsBlank
                        ? nil : candidate.draft?.album,
                    albumActions: albumActions,
                    records: candidate.records,
                    pressingUnknown: {
                        if case .album = candidate.releaseLink {
                            true
                        }
                        else {
                            false
                        }
                    }()
                ),
                tracks: ReleaseEditorTracks(
                    images: candidate.detail == nil ? [] : mapping.images,
                    onOpenImages: mappingActions.openImages,
                    sides: candidate.detail == nil ? [] : mapping.trackSides,
                    files: candidate.detail == nil ? [] : mapping.files,
                    previewingTarget: previewingTarget,
                    actions: mappingActions.trackTable
                ),
                editingCommands: editingCommands,
                editable: actionable,
                searching: searching,
                search: { onlineBrowser }
            )
        }
        else if searching {
            onlineBrowser
        }
        else {
            // The candidate's draft is a local read; nothing shows until it
            // lands.
            Color.clear
                .frame(maxWidth: .infinity, minHeight: 180)
        }
    }

    private var onlineBrowser: some View {
        ImportOnlineMetadataBrowser(
            candidateKey: candidate.key,
            endEditing: endEditing,
            onMovePane: onMovePane
        )
        .disabled(!actionable)
    }

    /// The card's commit row, absent while the draft is entirely blank.
    private var commitControls: ImportCommitControls? {
        guard candidate.detail != nil, !candidate.metadataDraftIsBlank else {
            return nil
        }
        return ImportCommitControls(
            untitledTrackCount: Int(mapping.untitledTrackCount),
            folderCheck: candidate.folderCheck,
            importStatus: candidate.importStatus,
            importDestination: $importDestination,
            actions: commitActions,
        )
    }

    @ViewBuilder
    private var banners: some View {
        if let detail = candidate.detail {
            ImportConfirmationBanners(
                libraryStatus: libraryStatus,
                importStatus: candidate.importStatus,
                error: candidate.error,
                failure: candidate.failure,
                canEdit: detail.actionable,
                onRetry: commitActions.confirmImport,
                onViewInLibrary: commitActions.viewInLibrary,
            )
        }
    }
}
