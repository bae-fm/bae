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

/// The identity section's card: the cover, what the release is, what
/// identified it, and the commit itself.
///
/// Search is this card's editor rather than a pane mounted beside it — the
/// change control opens it, and picking a release fills the mapping table's
/// BECOMES column in place. Before anything is picked the same control reads
/// "Find this release".
struct ImportReleaseHeader: View {
    let title: String
    let artist: String
    /// "CD · 1996 · 9 tracks", from what is being edited rather than what was
    /// fetched.
    let metaLine: String
    /// Whether a release has been picked.
    let hasPick: Bool
    /// Which service the picked release came from. `nil` before a pick and for
    /// a folder read as its own tags — there is no service behind either.
    let pickedSource: BridgeMetadataSource?
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
    let onFindRelease: () -> Void

    /// The cover the card leads with. Big enough to read the artwork as
    /// artwork — at a thumbnail's size it was an icon beside the title, and
    /// the cover is the thing being confirmed.
    private static let coverSize: CGFloat = 240

    @Environment(ConfigStore.self)
    private var configStore
    @State
    private var detailsExpanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 16) {
                cover
                summary
                changeControl
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

    /// The card's own fold: the release's fields, under a row that is the
    /// control end to end — a caret is a target the width of a glyph, and the
    /// line beside it says what opens.
    /// Where the picked release came from. The service's own name, never a
    /// code and never translated — it is a brand, and the name is what the
    /// person recognises.
    private func sourceChip(_ source: BridgeMetadataSource) -> some View {
        Text(verbatim: bridgeMetadataSourceName(source: source))
            .font(.system(size: 10.5, weight: .medium))
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(Color.secondary.opacity(0.15), in: Capsule())
            .foregroundStyle(.secondary)
            .lineLimit(1)
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
    private var changeControl: some View {
        HStack(spacing: 8) {
            ProgressView()
                .controlSize(.small)
                .opacity(isReading ? 1 : 0)
            if hasPick {
                Button(coreString("ui.import.header.change_release")) {
                    onFindRelease()
                }
                .buttonStyle(.bordered)
            }
            else {
                Button(coreString("ui.import.header.find_release")) {
                    onFindRelease()
                }
                .buttonStyle(.borderedProminent)
            }
        }
        .disabled(isReading)
    }

    private var summary: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.system(size: 17, weight: .semibold))
                .lineLimit(1)
                .truncationMode(.tail)
            Text(artist)
                .font(.system(size: 13))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            HStack(spacing: 6) {
                Text(metaLine)
                    .font(.system(size: 11.5))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                if let pickedSource {
                    sourceChip(pickedSource)
                }
            }
            .padding(.top, 4)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
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
