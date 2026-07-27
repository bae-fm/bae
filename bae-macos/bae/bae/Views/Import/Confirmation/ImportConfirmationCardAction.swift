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
        return false
    }

    private var completedAlbumId: String? {
        if case .complete(releaseId: _, albumId: let albumId) = importStatus {
            return albumId
        }
        return nil
    }

    /// Cloud-upload progress for the imported release, while its files are still
    /// queued. "Imported" means the rows are committed and the uploads durably
    /// queued — this surfaces the remaining transfer instead of presenting a
    /// cloud-only import as fully landed.
    private var uploadProgress: BridgeUploadProgress? {
        guard
            case .complete(releaseId: let releaseId, albumId: _) = importStatus
        else {
            return nil
        }
        return outboxStore.progress(forRelease: releaseId)
    }

    var body: some View {
        if isComplete {
            VStack(alignment: .trailing, spacing: 2) {
                if let progress = uploadProgress {
                    if progress.failed > 0 {
                        Label(
                            "Imported — upload failed, retrying",
                            systemImage: "exclamationmark.arrow.circlepath"
                        )
                        .foregroundStyle(.orange)
                        .font(.callout)
                        .help(
                            "The cloud upload failed and retries automatically. See the Storage Manager for details."
                        )
                    }
                    else {
                        VStack(alignment: .trailing, spacing: 4) {
                            Label(
                                "Imported — uploading to cloud",
                                systemImage: "icloud.and.arrow.up"
                            )
                            .foregroundStyle(.secondary)
                            .font(.callout)
                            ProgressTrackBar(progress: progress.fraction)
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
