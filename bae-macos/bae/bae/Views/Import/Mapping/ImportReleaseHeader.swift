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

enum ImportReleaseHeaderAction: Equatable {
    case changeRelease
    case useFileTags
    case enterManually

    var title: String {
        switch self {
        case .changeRelease:
            coreString("ui.import.header.change_release")
        case .useFileTags:
            coreString("ui.import.metadata.file_tags")
        case .enterManually:
            coreString("ui.import.metadata.manual")
        }
    }

    var isProminent: Bool {
        switch self {
        case .useFileTags, .enterManually: true
        case .changeRelease: false
        }
    }
}

/// The metadata-source section's card: the cover, what the release is, and the
/// commit itself.
///
/// Search is this card's editor rather than a pane mounted beside it — the
/// change control opens it, and picking a release fills the mapping table's
/// BECOMES column in place. Before anything is picked the same control reads
/// "Find this release" while Lookup is presented. File Tags omits that editor
/// action entirely.
struct ImportReleaseHeader: View {
    let releaseSummary: ImportReleaseSummary
    let action: ImportReleaseHeaderAction?
    /// Whether a read is in flight — the change control says so and stays put
    /// rather than the card being replaced by a placeholder.
    let isReading: Bool
    let coverContent: ImageContent?
    let hasCoverOptions: Bool
    /// The release's own fields, folded away at the card's foot: the card
    /// states what they add up to, and this is where a wrong year or a missing
    /// catalog number gets fixed before it is written. `nil` when there is no
    /// release to edit.
    let editValues: BridgeRawReleaseEdit?
    /// Where a typed field's value goes.
    let editActions: ReleaseFieldWriter
    /// The commit row at the card's foot. `nil` while there is nothing to
    /// commit — a failed re-pick leaves the fields in place but nothing
    /// settled to commit them under.
    let commit: ImportCommitControls?
    let onEditCover: () -> Void
    let onAction: () -> Void

    /// The cover the card leads with. Big enough to read the artwork as
    /// artwork — at a thumbnail's size it was an icon beside the title, and
    /// the cover is the thing being confirmed.
    private static let coverSize: CGFloat = 160

    @Environment(ConfigStore.self)
    private var configStore
    @State
    private var detailsExpanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 16) {
                cover
                ImportReleaseSummaryView(
                    summary: releaseSummary,
                    style: .card
                )
                actionControl
            }
            if let editValues {
                details(editValues)
            }
            if let commit {
                commitRow(commit)
            }
        }
        .padding(14)
        .formGroupCard()
    }

    private func details(
        _ values: BridgeRawReleaseEdit
    ) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Button {
                detailsExpanded.toggle()
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .rotationEffect(.degrees(detailsExpanded ? 90 : 0))
                    Text("Details")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if detailsExpanded {
                ReleaseFieldsForm(values: values, writer: editActions)
            }
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

    /// The card's editor, opened. Prominent while nothing is picked — it is the
    /// one thing left to do — and quiet once a release is in. While a read is
    /// in flight it goes quiet with a spinner beside it: the pane keeps showing
    /// what it already has.
    private var actionControl: some View {
        HStack(spacing: 8) {
            ProgressView()
                .controlSize(.small)
                .opacity(isReading ? 1 : 0)
            if let action {
                if action.isProminent {
                    Button(action.title) {
                        onAction()
                    }
                    .buttonStyle(.borderedProminent)
                }
                else {
                    Button(action.title) {
                        onAction()
                    }
                    .buttonStyle(.bordered)
                }
            }
        }
        .disabled(isReading)
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
    }
}

#if DEBUG

    #Preview("Import release header") {
        ImportReleaseHeader(
            releaseSummary: ImportReleaseSummary(
                candidate: PreviewData.mappingCandidate,
                editValues: PreviewData.confirmEditValues
            ),
            action: .changeRelease,
            isReading: false,
            coverContent: nil,
            hasCoverOptions: true,
            editValues: PreviewData.confirmEditValues,
            editActions: ReleaseFieldWriter { _, _ in },
            commit: nil,
            onEditCover: {},
            onAction: {},
        )
        .padding(24)
        .frame(width: 900, height: 360)
        .importPreviewEnvironment()
        .environment(Library.stub())
        .candidateReaderPreviewEnvironment()
    }

#endif
