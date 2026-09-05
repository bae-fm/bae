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

/// The editable metadata draft card: where the draft's metadata comes from,
/// then the cover beside the album heading and its release fields, then the
/// commit row once there is something to commit.
struct ImportReleaseHeader: View {
    let releaseSummary: ImportReleaseSummary
    /// Whether a read is in flight — the source controls wait for it rather
    /// than the card being replaced by a placeholder.
    let isReading: Bool
    let coverContent: ImageContent?
    /// Whether there is any artwork to pick from — the release's images or the
    /// folder's. Without one, the cover well says so instead of inviting a
    /// pick.
    let hasCoverOptions: Bool
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
    /// artwork — the cover is the thing being confirmed, and the heading
    /// beside it is sized to match.
    static let coverSize = ReleaseMetadataHeader<
        EmptyView, EmptyView, EmptyView
    >
    .coverSize

    @Environment(ConfigStore.self)
    private var configStore
    @State
    private var confirmsClear = false
    @State
    private var coverDropTargeted = false
    @State
    private var coverHovering = false

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            actionRow
            if let editValues {
                ReleaseMetadataHeader(
                    values: editValues,
                    writer: editActions,
                    editingCommands: editingCommands,
                    cover: { cover },
                    context: {
                        ImportReleaseContextView(summary: releaseSummary)
                    },
                    sourceAudio: {
                        if let sourceAudio = releaseSummary.sourceAudio {
                            ImportSourceAudioSummaryView(
                                sourceAudio: sourceAudio
                            )
                        }
                    }
                )
            }
        }
        .padding(16)
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

    /// The card's one row of actions: where the draft's metadata comes from
    /// on the left and, once there is something to commit, the commit on the
    /// right — storage, the unanswered tally, and the Import action.
    ///
    /// The source controls are one constant set in every draft state: both
    /// sources replace the same draft, so neither is promoted over the other,
    /// and clearing sits behind the ellipsis.
    private var actionRow: some View {
        HStack(alignment: .center, spacing: 16) {
            HStack(spacing: 8) {
                Button("Find release…") {
                    sourceActions.findOnline()
                }
                .buttonStyle(.bordered)
                Button("Use file metadata") {
                    sourceActions.useFileTags()
                }
                .buttonStyle(.bordered)
                clearMetadataMenu
            }
            .disabled(isReading)
            Spacer(minLength: 12)
            if let commit {
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
    }

    /// Whether the import already ran or is running — the storage choice is
    /// spent then, so its toggles leave the row.
    private func commitSettled(_ commit: ImportCommitControls) -> Bool {
        switch commit.importStatus {
        case .importing, .complete: return true
        case .error, nil: return false
        }
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
        .menuStyle(.button)
        .buttonStyle(.bordered)
        .menuIndicator(.hidden)
        .fixedSize()
    }

    /// The cover, or the well it goes in. The well invites the two ways a
    /// cover arrives — dragging one of the folder's images onto it, or picking
    /// from the release's and the folder's — and says when there is neither.
    private var cover: some View {
        Group {
            if let coverContent {
                ImageView(content: coverContent, pointSize: Self.coverSize)
            }
            else {
                artworkWell
            }
        }
        .frame(width: Self.coverSize, height: Self.coverSize)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(alignment: .topTrailing) {
            if coverContent != nil, hasCoverOptions {
                Image(systemName: "pencil")
                    .font(.caption2)
                    .foregroundStyle(.white)
                    .padding(3)
                    .background(.black.opacity(0.5))
                    .clipShape(RoundedRectangle(cornerRadius: 3))
                    .padding(4)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture {
            if hasCoverOptions {
                onEditCover()
            }
        }
        .onHover { coverHovering = $0 }
        .overlay {
            RoundedRectangle(cornerRadius: 8)
                .stroke(
                    Theme.accent,
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

    private var artworkWell: some View {
        let inviting = hasCoverOptions && coverHovering
        return VStack(spacing: 4) {
            if hasCoverOptions {
                Text("Add artwork")
                    .font(.system(size: 12, weight: .semibold))
                Text("Drag an image here, or click to choose")
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            else {
                Text("No artwork")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.secondary)
            }
        }
        .padding(16)
        .frame(width: Self.coverSize, height: Self.coverSize)
        .background(
            inviting ? Theme.accent.opacity(0.06) : Theme.hover
        )
        .overlay {
            RoundedRectangle(cornerRadius: 8)
                .strokeBorder(style: StrokeStyle(lineWidth: 1, dash: [4, 4]))
                .foregroundStyle(
                    inviting
                        ? AnyShapeStyle(Theme.accent)
                        : AnyShapeStyle(Color.primary.opacity(0.16))
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
            isReading: false,
            coverContent: nil,
            hasCoverOptions: true,
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
        .frame(width: 900, height: 420)
        .importPreviewEnvironment()
        .environment(Library.stub())
        .candidateReaderPreviewEnvironment()
    }

#endif
