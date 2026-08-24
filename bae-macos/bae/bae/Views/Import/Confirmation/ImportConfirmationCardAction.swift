import BaeKit
import SwiftUI

/// The trailing action cell of the commit bar: the `Import` button before
/// commit, the running import's step, percent and bar while importing, a `Retry
/// Import` button on error, and the `Imported` / cloud-upload state once
/// complete.
///
/// The button is never disabled. An edit bae-core cannot shape into a savable
/// release is refused at commit and the reason is stated on the pane — a
/// disabled button that says nothing is the thing that redesign removed.
struct ImportConfirmationCardAction: View {
    /// Where the candidate's import stands, as its row places it.
    let importStatus: BridgeTriageImportStatus?
    /// Routes the running import's progress to the leaf line that draws it.
    let candidateKey: String
    let onConfirmImport: () -> Void
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

    var body: some View {
        if isComplete {
            VStack(alignment: .trailing, spacing: 2) {
                if case .active(let progress) = uploadObservation {
                    VStack(alignment: .trailing, spacing: 4) {
                        UploadActivityLabel(progress: progress)
                            .font(.callout)
                        if let bar = progress.bar {
                            Text(bar.text)
                                .font(.caption)
                                .monospacedDigit()
                                .foregroundStyle(.secondary)
                            ProgressTrackBar(progress: bar.fraction)
                                .frame(width: 160)
                        }
                    }
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
                ImportProgressLine(key: candidateKey)
                    .frame(width: 200)
            case .error:
                Button("Retry Import") { onConfirmImport() }
                    .buttonStyle(.borderedProminent)
            case .complete:
                EmptyView()
            }
        }
        else {
            Button("Import") { onConfirmImport() }
                .buttonStyle(.borderedProminent)
        }
    }
}

#if DEBUG
    #Preview("Card action — ready") {
        ImportConfirmationCardAction(
            importStatus: nil,
            candidateKey: "preview-candidate",
            onConfirmImport: {},
            onViewInLibrary: { _ in },
        )
        .padding()
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .windowBackground()
    }
#endif
