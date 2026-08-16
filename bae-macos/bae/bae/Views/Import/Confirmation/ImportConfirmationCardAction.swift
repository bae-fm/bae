import BaeKit
import SwiftUI

/// The trailing action cell of the commit bar: the `Import` button before
/// commit, live progress (a determinate loudness bar during the loudness pass,
/// an indeterminate spinner + step label otherwise) while importing, a `Retry
/// Import` button on error, and the `Imported` / cloud-upload state once
/// complete.
///
/// The button is never disabled. An edit bae-core cannot shape into a savable
/// release is refused at commit and the reason is stated on the pane — a
/// disabled button that says nothing is the thing that redesign removed.
struct ImportConfirmationCardAction: View {
    let importStatus: BridgeCandidateImportStatus?
    /// Routes the high-frequency loudness ticks to the leaf bar during the
    /// measuring-loudness phase.
    let candidateKey: String
    let onConfirmImport: () -> Void
    let onViewInLibrary: (String) -> Void

    @Environment(OutboxStore.self)
    private var outboxStore

    private var isComplete: Bool {
        if case .complete = importStatus {
            return true
        }
        if case .cloudUploadQueued = importStatus {
            return true
        }
        return false
    }

    private var completedAlbumId: String? {
        switch importStatus {
        case .complete(releaseId: _, let albumId),
            .cloudUploadQueued(
                releaseId: _,
                let albumId,
                outboxRevision: _
            ):
            return albumId
        default:
            return nil
        }
    }

    /// The imported release's durable cloud-transition observation. This keeps
    /// the import result truthful across bridge delivery ordering: awaiting the
    /// first queue snapshot is still Queued, and Imported appears only after a
    /// previously observed transition leaves the outbox.
    private var uploadObservation: UploadObservation? {
        switch importStatus {
        case .cloudUploadQueued(
            let releaseId,
            albumId: _,
            let outboxRevision
        ):
            return outboxStore.uploadObservation(
                forRelease: releaseId,
                queuedAtRevision: outboxRevision
            )
        case .complete(let releaseId, albumId: _):
            return outboxStore.persistedUploadObservation(
                forRelease: releaseId
            )
        default:
            return nil
        }
    }

    var body: some View {
        if isComplete {
            VStack(alignment: .trailing, spacing: 2) {
                if case .active(let progress) = uploadObservation {
                    VStack(alignment: .trailing, spacing: 4) {
                        UploadActivityLabel(progress: progress)
                            .font(.callout)
                        if let stageBytesText = progress.stageBytesText {
                            Text(stageBytesText)
                                .font(.caption)
                                .monospacedDigit()
                                .foregroundStyle(.secondary)
                        }
                        if progress.workTotal > 0 {
                            ProgressTrackBar(progress: progress.fraction)
                                .frame(width: 160)
                        }
                    }
                }
                else if case .awaiting = uploadObservation {
                    Label("Queued", systemImage: "clock")
                        .foregroundStyle(.secondary)
                        .font(.callout)
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
            case .importing(_, let step):
                if case .running(.measuringLoudness)? = step {
                    // The loudness pass is the long pole; show its live,
                    // determinate per-track bar (updated imperatively off the
                    // high-frequency signal) instead of an indeterminate spinner.
                    ImportLoudnessProgressRepresentable(key: candidateKey)
                        .frame(width: 200, height: 32)
                }
                else {
                    HStack(spacing: 6) {
                        ProgressView()
                            .controlSize(.small)
                        if let step {
                            Text(step.localizedText)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                        }
                    }
                }
            case .error:
                Button("Retry Import") { onConfirmImport() }
                    .buttonStyle(.borderedProminent)
            case .complete, .cloudUploadQueued:
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
