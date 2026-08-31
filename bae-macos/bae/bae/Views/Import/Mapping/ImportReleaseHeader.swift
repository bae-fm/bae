import BaeKit
import SwiftUI

/// The commit controls the card carries once there is something to commit:
/// storage, the Import action, and what is still unanswered. Nothing here
/// disables the commit — the count is a statement, and the one refusal left
/// in the whole import is audio that will not decode, which core raises.
struct ImportCommitControls {
    let unansweredCount: Int
    /// Routes the running import's progress to the leaf line that draws it.
    let candidateKey: String
    /// Where the candidate's import stands, as its row places it.
    let importStatus: BridgeTriageImportStatus?
    let storageCloud: Binding<Bool>
    let storagePinned: Binding<Bool>
    let actions: ImportCommitActions
}

struct ImportReleaseSourceActions {
    let findOnline: () -> Void
    let useFileTags: () -> Void
    let clearMetadata: () -> Void
}

/// The import card's album identity retains the summary's hierarchy while
/// making each value directly editable.
struct ImportAlbumIdentityEditor: View {
    let values: BridgeRawReleaseEdit
    let summary: ImportReleaseSummary
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            CommittedTextField(
                placeholder: String(localized: "Album title"),
                value: values.albumTitle,
                boxed: false,
                font: .system(size: 17, weight: .semibold),
                editingCommands: editingCommands,
                onCommit: {
                    await writer.setField(.albumTitle, $0)
                },
            )
            ArtistAssignmentsField(
                assignments: values.albumArtistAssignments,
                placeholder: String(localized: "Album artist"),
                onChange: { assignments in
                    Task { await writer.setAlbumArtists(assignments) }
                },
            )
            .font(.system(size: 13))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            HStack(spacing: 6) {
                CommittedTextField(
                    placeholder: String(localized: "Year"),
                    value: values.albumYear,
                    monospaced: true,
                    boxed: false,
                    font: .system(size: 11.5),
                    editingCommands: editingCommands,
                    onCommit: {
                        await writer.setField(.albumYear, $0)
                    },
                )
                .foregroundStyle(.tertiary)
                .frame(width: 64)
                ImportReleaseContextView(summary: summary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The disclosure that keeps the editable release fields attached to the
/// metadata text column.
struct ImportReleaseDetails: View {
    let values: BridgeRawReleaseEdit
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands
    @Binding
    var expanded: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Button {
                expanded = !expanded
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .rotationEffect(.degrees(expanded ? 90 : 0))
                    Text("Details")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if expanded {
                ReleaseFieldsForm(
                    values: values,
                    writer: writer,
                    sections: [.pressing],
                    editingCommands: editingCommands
                )
            }
        }
    }
}

/// The editable metadata draft card: cover, release fields, source provenance,
/// replacement actions, and commit controls.
struct ImportReleaseHeader: View {
    let releaseSummary: ImportReleaseSummary
    let draftIsBlank: Bool
    /// Whether a read is in flight — the change control says so and stays put
    /// rather than the card being replaced by a placeholder.
    let isReading: Bool
    let coverContent: ImageContent?
    let hasCoverOptions: Bool
    @Binding
    var detailsExpanded: Bool
    /// The album fields stay visible; pressing fields fold under Details.
    /// `nil` when there is no release to edit.
    let editValues: BridgeRawReleaseEdit?
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

    /// The cover the card leads with. Big enough to read the artwork as
    /// artwork — at a thumbnail's size it was an icon beside the title, and
    /// the cover is the thing being confirmed.
    static let coverSize: CGFloat = 160

    @Environment(ConfigStore.self)
    private var configStore
    @State
    private var confirmsClear = false
    @State
    private var coverDropTargeted = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 16) {
                coverColumn
                VStack(alignment: .leading, spacing: 12) {
                    sourceActionControl
                    if let editValues {
                        ImportAlbumIdentityEditor(
                            values: editValues,
                            summary: releaseSummary,
                            writer: editActions,
                            editingCommands: editingCommands
                        )
                    }
                    detailsSection
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            if let commit {
                commitRow(commit)
            }
        }
        .padding(14)
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
    }

    /// Storage, the unanswered tally when there is one, and the Import action
    /// — the commit lives on the card that states what will be committed.
    private func commitRow(_ commit: ImportCommitControls) -> some View {
        HStack(alignment: .center, spacing: 16) {
            if commit.unansweredCount > 0 {
                Text(
                    coreString(
                        "ui.import.commit.unanswered",
                        commit.unansweredCount
                    )
                )
                .font(.system(size: 11.5))
                .foregroundStyle(.orange)
            }
            Spacer(minLength: 12)
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
                onConfirmImport: commit.actions.confirmImport,
                onViewInLibrary: commit.actions.viewInLibrary,
            )
        }
    }

    /// Whether the import already ran or is running — the storage choice is
    /// spent then, so its toggles leave the row.
    private func commitSettled(_ commit: ImportCommitControls) -> Bool {
        switch commit.importStatus {
        case .importing, .complete: return true
        case .error, nil: return false
        }
    }

    /// Metadata sources stay together: both replace the same draft by choosing
    /// where its metadata comes from.
    private var sourceActionControl: some View {
        VStack(alignment: .leading, spacing: 5) {
            FormEyebrow(text: Text("Metadata"))
            HStack(spacing: 8) {
                ProgressView()
                    .controlSize(.small)
                    .opacity(isReading ? 1 : 0)
                if draftIsBlank, !releaseSummary.hasMatchedRelease {
                    findOnlineButton.buttonStyle(.borderedProminent)
                }
                else {
                    findOnlineButton.buttonStyle(.bordered)
                }
                if !releaseSummary.hasMatchedRelease {
                    Button("Use file metadata") {
                        sourceActions.useFileTags()
                    }
                    .buttonStyle(.bordered)
                    if !draftIsBlank {
                        clearMetadataMenu
                    }
                }
            }
        }
        .disabled(isReading)
    }

    private var findOnlineButton: some View {
        Button {
            sourceActions.findOnline()
        } label: {
            if releaseSummary.hasMatchedRelease {
                Text("Change release…")
            }
            else {
                Text("Match release…")
            }
        }
    }

    @ViewBuilder
    private var detailsSection: some View {
        if let editValues {
            ImportReleaseDetails(
                values: editValues,
                writer: editActions,
                editingCommands: editingCommands,
                expanded: $detailsExpanded
            )
        }
        if releaseSummary.hasMatchedRelease, detailsExpanded {
            matchedReleaseSecondaryActions
        }
    }

    private var matchedReleaseSecondaryActions: some View {
        HStack(spacing: 8) {
            Button("Use file metadata") {
                sourceActions.useFileTags()
            }
            .buttonStyle(.bordered)
            clearMetadataMenu
        }
        .disabled(isReading)
    }

    private var clearMetadataMenu: some View {
        Menu {
            Button("Clear metadata", role: .destructive) {
                confirmsClear = true
            }
        } label: {
            Image(systemName: "ellipsis")
                .accessibilityLabel(Text("Clear metadata"))
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }

    private var cover: some View {
        Group {
            if let coverContent {
                ImageView(content: coverContent, pointSize: Self.coverSize)
            }
            else {
                Theme.placeholder
            }
        }
        .frame(width: Self.coverSize, height: Self.coverSize)
        .clipShape(RoundedRectangle(cornerRadius: 6))
        .overlay(alignment: .topTrailing) {
            if hasCoverOptions {
                Image(systemName: "pencil")
                    .font(.caption2)
                    .foregroundStyle(.white)
                    .padding(3)
                    .background(.black.opacity(0.5))
                    .clipShape(RoundedRectangle(cornerRadius: 3))
                    .padding(2)
            }
        }
        .onTapGesture {
            if hasCoverOptions {
                onEditCover()
            }
        }
        .overlay {
            RoundedRectangle(cornerRadius: 6)
                .stroke(
                    Color.accentColor,
                    lineWidth: coverDropTargeted ? 3 : 0
                )
        }
        .dropDestination(for: String.self) { fileIds, _ in
            guard let fileId = fileIds.first,
                let selection = localCoverSelections[fileId]
            else { return false }
            onSelectCover(selection)
            return true
        } isTargeted: {
            coverDropTargeted = $0
        }
    }

    private var coverColumn: some View {
        VStack(alignment: .leading, spacing: 6) {
            cover
            if let sourceAudio = releaseSummary.sourceAudio {
                ImportSourceAudioSummaryView(sourceAudio: sourceAudio)
                    .frame(width: Self.coverSize, alignment: .leading)
            }
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
            draftIsBlank: false,
            isReading: false,
            coverContent: nil,
            hasCoverOptions: true,
            detailsExpanded: .constant(false),
            editValues: PreviewData.confirmEditValues,
            editActions: ReleaseFieldWriter { _, _ in },
            editingCommands: EditingCommitCommands(),
            commit: nil,
            sourceActions: ImportReleaseSourceActions(
                findOnline: {},
                useFileTags: {},
                clearMetadata: {}
            ),
            localCoverSelections: [:],
            onEditCover: {},
            onSelectCover: { _ in },
        )
        .padding(24)
        .frame(width: 900, height: 360)
        .importPreviewEnvironment()
        .environment(Library.stub())
        .candidateReaderPreviewEnvironment()
    }

#endif
