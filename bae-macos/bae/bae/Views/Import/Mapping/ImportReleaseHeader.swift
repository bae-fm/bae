import BaeKit
import SwiftUI

/// The card's commit controls: the folder check, the unanswered count,
/// storage, and Import, which nothing here disables.
struct ImportCommitControls {
    let unansweredCount: Int
    /// Why the folder keeps this candidate out of a bulk import, such as a
    /// release that lists no tracks; shown beside Import, which stays
    /// available.
    let folderCheck: BridgeFolderCheck?
    /// What the last import left: a failure, which Import retries, or
    /// nothing.
    let importStatus: BridgeCandidateImportStatus?
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
    /// Unlink the candidate from its release, leaving the draft as it is.
    let unlink: () -> Void
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
    /// Every catalog that describes the release the candidate is linked to.
    /// Empty for a candidate linked to no release.
    let records: [BridgeReleaseRecord]
    /// What the candidate is linked to: an album link says its pressing is
    /// unknown beside the album's records.
    let releaseLink: BridgeReleaseLink?
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
                // The catalogs that describe the linked release end the card,
                // under their own divider, beside the way to unlink it.
                if !records.isEmpty {
                    Rectangle()
                        .fill(Theme.hairline)
                        .frame(height: 1)
                    HStack(alignment: .firstTextBaseline) {
                        ReleaseRecordsRow(records: records, scale: .pane)
                        if case .album = releaseLink {
                            Text("Pressing unknown")
                                .themeText(.detail)
                                .foregroundStyle(.secondary)
                                .fixedSize()
                        }
                        Spacer(minLength: ThemeSpace.group)
                        Button("Unlink") {
                            sourceActions.unlink()
                        }
                        .buttonStyle(.link)
                        .help(
                            "Stop treating the folder as this release. The metadata stays as it is."
                        )
                        .disabled(!actionable)
                    }
                }
            }
        }
        .padding(.vertical, ThemeSpace.edge)
        .padding(.horizontal, ThemeSpace.section)
        .card()
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

    /// The identify entries and metadata menu, then the commit controls once
    /// there is something to commit; only Automatic starts a run.
    private var actionRow: some View {
        HStack(alignment: .center, spacing: ThemeSpace.edge) {
            HStack(spacing: ThemeSpace.related) {
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
            Spacer(minLength: ThemeSpace.group)
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
                if configStore.config.hasCloudHome {
                    HStack(spacing: ThemeSpace.related) {
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
                    onConfirmImport: commit.actions.confirmImport
                )
            }
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
                    .themeIcon(.badge)
                    .foregroundStyle(Theme.onFill)
                    .padding(ThemeSpace.line)
                    .background(Theme.scrim)
                    .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.chip))
                    .padding(ThemeSpace.inline)
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
        return VStack(spacing: ThemeSpace.inline) {
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
        .padding(ThemeSpace.related)
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
            releaseLink: nil,
            editActions: ReleaseFieldWriter { _, _ in },
            editingCommands: EditingCommitCommands(),
            commit: nil,
            sourceActions: ImportReleaseSourceActions(
                identifyAutomatically: {},
                searchForRelease: {},
                reset: {},
                resetToFileMetadata: {},
                clearMetadata: {},
                unlink: {}
            ),
            localCoverSelections: [:],
            onEditCover: {},
            onSelectCover: { _ in },
        )
        .padding(ThemeSpace.section)
        .frame(width: 900, height: 620)
        .importPreviewEnvironment()
        .environment(Library.stub())
        .candidateReaderPreviewEnvironment()
    }

#endif
