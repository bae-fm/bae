import BaeKit
import SwiftUI

/// The card's commit controls: the folder check, the unanswered count,
/// storage, and Import, which nothing here disables.
struct ImportCommitControls {
    let unansweredCount: Int
    /// Why the folder keeps this candidate out of a bulk import, such as a
    /// track count mismatch; shown beside Import, which stays available.
    let folderCheck: BridgeFolderCheck?
    /// Routes the running import's progress to the leaf line that draws it.
    let candidateKey: String
    /// The running import, or the result of the last one.
    let importStatus: BridgeCandidateImportStatus?
    /// Whether core still allows cancelling the running import, which it does
    /// until the release is being written.
    let canCancelImport: Bool
    let storageCloud: Binding<Bool>
    let storagePinned: Binding<Bool>
    let actions: ImportCommitActions
}

struct ImportReleaseSourceActions {
    /// Identify the candidate now: open the pane and start a fresh run.
    let identifyAutomatically: () -> Void
    /// Open the same pane on its typed search, starting nothing.
    let searchForRelease: () -> Void
    /// Restore the initial source tracks, audio assignments, cover, and
    /// metadata.
    let reset: () -> Void
    /// Replace the draft with what the candidate's own files say.
    let resetToFileMetadata: () -> Void
    let clearMetadata: () -> Void
}

/// The editable metadata draft card: the action row, the cover beside the
/// release fields, and the catalogs that describe the release.
struct ImportReleaseHeader: View {
    let releaseSummary: ImportReleaseSummary
    /// Whether the draft can be edited and identified now; the records row
    /// stays enabled.
    let actionable: Bool
    /// Whether a read is in flight, which disables the identify controls.
    let isReading: Bool
    let coverContent: ImageContent?
    /// Whether the release or the folder has any artwork to pick from.
    let hasCoverOptions: Bool
    /// `nil` when there is no release to edit.
    let editValues: BridgeRawReleaseEdit?
    /// Every catalog that describes the release the draft was read from.
    /// Empty for a draft read from the files' own tags, or typed in.
    let records: [BridgeReleaseRecord]
    /// Where a typed field's value goes.
    let editActions: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands
    /// The commit row at the card's foot. `nil` while there is nothing to
    /// commit.
    let commit: ImportCommitControls?
    let sourceActions: ImportReleaseSourceActions
    let localCoverSelections: [String: BridgeCoverSelection]
    let onEditCover: () -> Void
    let onSelectCover: (BridgeCoverSelection) -> Void

    @Environment(ConfigStore.self)
    private var configStore
    @State
    private var confirmsClear = false
    @State
    private var confirmsReset = false
    @State
    private var confirmsResetToTags = false

    var body: some View {
        VStack(alignment: .leading, spacing: ReleaseMetadataLayout.blockSpacing)
        {
            actionRow
                .disabled(!actionable)
            if let editValues {
                ReleaseMetadataHeader(
                    values: editValues,
                    writer: editActions,
                    editingCommands: editingCommands,
                    cover: {
                        ImportCoverWell(
                            coverContent: coverContent,
                            hasCoverOptions: hasCoverOptions,
                            localCoverSelections: localCoverSelections,
                            onEditCover: onEditCover,
                            onSelectCover: onSelectCover
                        )
                    },
                    audioFacts: {
                        if let sourceAudio = releaseSummary.sourceAudio {
                            ImportSourceAudioSummaryView(
                                sourceAudio: sourceAudio
                            )
                        }
                    }
                )
                .disabled(!actionable)
                // The catalogs that describe the release end the card,
                // under their own divider.
                if !records.isEmpty {
                    Rectangle()
                        .fill(Theme.hairline)
                        .frame(height: 1)
                    ReleaseRecordsRow(records: records, scale: .pane)
                }
            }
        }
        .padding(.vertical, 16)
        .padding(.horizontal, 20)
        .formGroupCard()
        .confirmationDialog(
            "Clear metadata?",
            isPresented: $confirmsClear,
            titleVisibility: .visible
        ) {
            Button("Clear metadata", role: .destructive) {
                sourceActions.clearMetadata()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(
                "The candidate files and mapping choices will remain unchanged."
            )
        }
        .confirmationDialog(
            "Reset to file metadata?",
            isPresented: $confirmsResetToTags,
            titleVisibility: .visible
        ) {
            Button("Reset to file metadata", role: .destructive) {
                sourceActions.resetToFileMetadata()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(
                "Replace metadata with what the files, sheets and folder say about the release. Tracks and audio assignments will remain unchanged."
            )
        }
        .confirmationDialog(
            "Reset?",
            isPresented: $confirmsReset,
            titleVisibility: .visible
        ) {
            Button("Reset", role: .destructive) {
                sourceActions.reset()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(
                "Restore all tracks, automatic audio assignments, artwork, and initial metadata. Your files will not be changed."
            )
        }
    }

    /// The identify entries and metadata menu on the left, and the commit
    /// controls on the right once there is something to commit. Both entries
    /// open the same pane; only Automatic starts a run.
    private var actionRow: some View {
        HStack(alignment: .center, spacing: 16) {
            HStack(spacing: 8) {
                Text("Identify")
                    .themeText(.strong)
                    .foregroundStyle(.secondary)
                Button("Automatic") {
                    sourceActions.identifyAutomatically()
                }
                .buttonStyle(.bordered)
                Button("Search") {
                    sourceActions.searchForRelease()
                }
                .buttonStyle(.bordered)
                candidateMenu
            }
            .disabled(isReading)
            Spacer(minLength: 12)
            if let commit {
                if let folderCheck = commit.folderCheck?.localizedText {
                    Text(folderCheck)
                        .themeText(.detail)
                        .foregroundStyle(Theme.warning)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .help(folderCheck)
                }
                if commit.unansweredCount > 0 {
                    Text(
                        coreString(
                            "ui.import.commit.unanswered",
                            commit.unansweredCount
                        )
                    )
                    .themeText(.detail)
                    .foregroundStyle(Theme.warning)
                }
                if !commitSettled(commit), configStore.config.hasCloudHome {
                    HStack(spacing: 10) {
                        ImportCheckboxToggle(
                            "Cloud",
                            isOn: commit.storageCloud
                        )
                        if commit.storageCloud.wrappedValue {
                            ImportCheckboxToggle(
                                "Pinned",
                                isOn: commit.storagePinned
                            )
                        }
                    }
                    .fixedSize()
                }
                ImportConfirmationCardAction(
                    importStatus: commit.importStatus,
                    candidateKey: commit.candidateKey,
                    canCancelImport: commit.canCancelImport,
                    onConfirmImport: commit.actions.confirmImport,
                    onCancelImport: commit.actions.cancelImport,
                    onViewInLibrary: commit.actions.viewInLibrary,
                )
            }
        }
    }

    /// Whether the import already ran or is running, which hides the storage
    /// toggles.
    private func commitSettled(_ commit: ImportCommitControls) -> Bool {
        switch commit.importStatus {
        case .importing, .complete: return true
        case .error, nil: return false
        }
    }

    /// Commands that replace metadata or restore the whole import setup.
    private var candidateMenu: some View {
        Menu {
            Button("Reset", role: .destructive) {
                confirmsReset = true
            }
            Button("Reset to file metadata", role: .destructive) {
                confirmsResetToTags = true
            }
            Button("Clear metadata", role: .destructive) {
                confirmsClear = true
            }
        } label: {
            Image(systemName: "ellipsis")
                .accessibilityLabel(Text("Metadata"))
        }
        .menuStyle(.button)
        .buttonStyle(.bordered)
        .menuIndicator(.hidden)
        .fixedSize()
    }
}

/// The card's cover, or an empty well that invites dropping or picking one,
/// or says there is none.
struct ImportCoverWell: View {
    let coverContent: ImageContent?
    /// Whether the release or the folder has any artwork to pick from.
    let hasCoverOptions: Bool
    let localCoverSelections: [String: BridgeCoverSelection]
    let onEditCover: () -> Void
    let onSelectCover: (BridgeCoverSelection) -> Void

    static let coverSize = ReleaseMetadataLayout.coverSize

    @State
    private var dropTargeted = false
    @State
    private var hovering = false

    var body: some View {
        Group {
            if let coverContent {
                ImageView(content: coverContent, pointSize: Self.coverSize)
            }
            else {
                artworkWell
            }
        }
        .frame(width: Self.coverSize, height: Self.coverSize)
        .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
        .overlay(alignment: .topTrailing) {
            if coverContent != nil, hasCoverOptions {
                Image(systemName: "pencil")
                    .font(.caption2)
                    .foregroundStyle(Theme.onFill)
                    .padding(3)
                    .background(Theme.scrim)
                    .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.chip))
                    .padding(4)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture {
            if hasCoverOptions {
                onEditCover()
            }
        }
        .onHover { hovering = $0 }
        .overlay {
            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                .stroke(
                    Theme.accent,
                    lineWidth: dropTargeted ? 3 : 0
                )
        }
        .dropDestination(for: String.self) { fileIds, _ in
            guard let fileId = fileIds.first,
                let selection = localCoverSelections[fileId]
            else { return false }
            onSelectCover(selection)
            return true
        } isTargeted: {
            dropTargeted = $0
        }
    }

    private var artworkWell: some View {
        let inviting = hasCoverOptions && hovering
        return VStack(spacing: 4) {
            if hasCoverOptions {
                Text("Add artwork")
                    .themeText(.strong)
                Text("Drag an image here, or click to choose")
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            else {
                Text("No artwork")
                    .themeText(.strong)
                    .foregroundStyle(.secondary)
            }
        }
        // The hint wraps to two lines in the width the cover leaves it.
        .padding(10)
        .frame(width: Self.coverSize, height: Self.coverSize)
        .background(
            inviting ? Theme.accentSoft : Theme.hover
        )
        .overlay {
            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                .strokeBorder(style: StrokeStyle(lineWidth: 1, dash: [4, 4]))
                .foregroundStyle(
                    inviting
                        ? AnyShapeStyle(Theme.accent)
                        : AnyShapeStyle(Theme.hairlineStrong)
                )
        }
    }
}

#if DEBUG

    #Preview("Import release header") {
        ImportReleaseHeader(
            releaseSummary: ImportReleaseSummary(
                candidate: PreviewData.mappingCandidate,
                editValues: PreviewData.confirmEditValues
            ),
            actionable: true,
            isReading: false,
            coverContent: nil,
            hasCoverOptions: true,
            editValues: PreviewData.confirmEditValues,
            records: PreviewData.releaseRecordsPair,
            editActions: ReleaseFieldWriter { _, _ in },
            editingCommands: EditingCommitCommands(),
            commit: nil,
            sourceActions: ImportReleaseSourceActions(
                identifyAutomatically: {},
                searchForRelease: {},
                reset: {},
                resetToFileMetadata: {},
                clearMetadata: {}
            ),
            localCoverSelections: [:],
            onEditCover: {},
            onSelectCover: { _ in },
        )
        .padding(24)
        .frame(width: 900, height: 620)
        .importPreviewEnvironment()
        .environment(Library.stub())
        .candidateReaderPreviewEnvironment()
    }

#endif
