import BaeKit
import SwiftUI

/// The commit controls the card carries once there is something to commit:
/// storage, the Import action, and what is still unanswered. Nothing here
/// disables the commit — the count is a statement, and the one refusal left
/// in the whole import is audio that will not decode, which core raises.
struct ImportCommitControls {
    let unansweredCount: Int
    /// Routes the loudness ticks to the leaf progress bar during the loudness
    /// pass.
    let candidateKey: String
    let importStatus: BridgeCandidateImportStatus?
    let storageManaged: Binding<Bool>
    let storagePinned: Binding<Bool>
    let actions: ImportCommitActions
}

/// The identity section's card: the cover, what the release is, the claim
/// this import records, and the commit itself.
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
    /// What this import claims to hold and where its metadata came from, as
    /// core reads it back off the stored pick. `nil` before a pick, and for an
    /// Unknown import, which claims nothing.
    let claim: BridgeClaimLine?
    /// Whether a release has been picked.
    let hasPick: Bool
    /// Whether a read is in flight — the change control says so and stays put
    /// rather than the card being replaced by a placeholder.
    let isReading: Bool
    let coverContent: ImageContent?
    let hasCoverOptions: Bool
    /// The commit row at the card's foot. `nil` while there is nothing to
    /// commit — a failed re-pick leaves the fields in place but nothing
    /// settled to commit them under.
    let commit: ImportCommitControls?
    let onEditCover: () -> Void
    let onFindRelease: () -> Void
    /// Set how far the claim reaches. Re-picks the same release at the level
    /// given, which is what stores it.
    let onSetClaimLevel: (BridgeClaimLevel) -> Void

    @Environment(ConfigStore.self)
    private var configStore

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 16) {
                cover
                summary
                changeControl
            }
            if let claim {
                ImportClaimLine(
                    claim: claim,
                    isReading: isReading,
                    onSetLevel: onSetClaimLevel,
                )
            }
            if let commit {
                commitRow(commit)
            }
        }
        .padding(14)
        .formGroupCard()
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
                        "Managed",
                        isOn: commit.storageManaged
                    )
                    if commit.storageManaged.wrappedValue {
                        ImportCheckboxToggle(
                            "Keep local copy",
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
            Text(metaLine)
                .font(.system(size: 11.5))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .padding(.top, 4)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var cover: some View {
        Group {
            if let coverContent {
                ImageView(content: coverContent, pointSize: 80)
            }
            else {
                Theme.placeholder
            }
        }
        .frame(width: 80, height: 80)
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
