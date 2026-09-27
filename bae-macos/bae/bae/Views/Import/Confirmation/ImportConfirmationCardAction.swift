import BaeKit
import SwiftUI

/// The trailing action cell of the commit bar: the `Import` button before
/// commit; while importing, the running import's step and bar with the cancel
/// where `Import` was; a `Retry Import` button on error; and the `Imported` /
/// cloud-upload state once complete.
///
/// `Import` is never disabled. An edit bae-core cannot shape into a savable
/// release is refused at commit and the reason is stated on the pane — a
/// disabled button that says nothing is the thing that redesign removed.
struct ImportConfirmationCardAction: View {
    /// Where the candidate's import stands, as its row places it.
    let importStatus: BridgeCandidateImportStatus?
    /// Routes the running import's progress to the leaf line that draws it.
    let candidateKey: String
    /// Whether core offers to cancel the running import — until it begins
    /// writing its release, the same answer the row's menu offers from.
    let canCancelImport: Bool
    let onConfirmImport: () -> Void
    let onCancelImport: () -> Void
    let onViewInLibrary: (String) -> Void

    @Environment(OutboxStore.self)
    private var outboxStore

    private var isComplete: Bool {
        if case .complete = importStatus {
            return true
        }
        return false
    }

    private var completedAlbumId: String? {
        guard case .complete(releaseId: _, let albumId) = importStatus else {
            return nil
        }
        return albumId
    }

    /// The imported release's cloud transition, where the outbox holds one. A
    /// release with nothing queued is absent from the outbox, which is what
    /// "the import is done" reads as here.
    private var uploadObservation: UploadObservation? {
        guard case .complete(let releaseId, albumId: _) = importStatus else {
            return nil
        }
        return outboxStore.persistedUploadObservation(forRelease: releaseId)
    }

    /// The running import's cancel, in the place `Import` held. An import
    /// writing its release can no longer be cancelled, and the button says
    /// so instead of offering a cancel core would refuse.
    @ViewBuilder
    private var cancelButton: some View {
        if canCancelImport {
            Button("Cancel Import") { onCancelImport() }
        }
        else {
            Button("Finishing\u{2026}") {}
                .disabled(true)
                .help(
                    String(
                        localized:
                            "The release is being written and can no longer be cancelled."
                    )
                )
        }
    }

    var body: some View {
        if isComplete {
            VStack(alignment: .trailing, spacing: 2) {
                if case .active(let progress) = uploadObservation {
                    ProgressLine(progress: progress.bar?.fraction) {
                        UploadActivityLabel(progress: progress)
                    }
                    .font(.callout)
                    .frame(width: 200)
                }
                else {
                    Label("Imported", systemImage: "checkmark.circle.fill")
                        .foregroundStyle(.green)
                        .font(.callout)
                }
                if let albumId = completedAlbumId {
                    Button("View in Library") { onViewInLibrary(albumId) }
                        .buttonStyle(.link)
                        .font(.callout)
                }
            }
        }
        else if let status = importStatus {
            switch status {
            case .importing:
                // The step, how far through the whole import it is, and the
                // bar — the same component the candidate's row draws, off the
                // same signal, so the two surfaces cannot come to disagree
                // about one run.
                HStack(spacing: 12) {
                    ImportProgressLine(key: candidateKey)
                        .frame(width: 200)
                    cancelButton
                }
            case .error:
                Button("Retry Import") { onConfirmImport() }
                    .buttonStyle(PrimaryButtonStyle())
            case .complete:
                EmptyView()
            }
        }
        else {
            Button("Import") { onConfirmImport() }
                .buttonStyle(PrimaryButtonStyle())
        }
    }
}

#if DEBUG
    #Preview("Card action — ready") {
        ImportConfirmationCardAction(
            importStatus: nil,
            candidateKey: "preview-candidate",
            canCancelImport: false,
            onConfirmImport: {},
            onCancelImport: {},
            onViewInLibrary: { _ in },
        )
        .padding()
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        // A running import draws its progress through the candidate-runtime
        // reader, so the preview mounts the same chain the app does even while
        // it is showing the state before one starts.
        .candidateReaderPreviewEnvironment()
        .windowBackground()
    }
#endif
