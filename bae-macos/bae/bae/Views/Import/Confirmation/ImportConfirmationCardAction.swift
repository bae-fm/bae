import BaeKit
import SwiftUI

/// The commit bar's trailing action: `Import`, the running import's progress
/// and cancel, `Retry Import` on error, or the imported and uploading state.
///
/// `Import` is never disabled; an edit core cannot save is refused at commit
/// and the pane states why.
struct ImportConfirmationCardAction: View {
    /// Where the candidate's import stands, as its row places it.
    let importStatus: BridgeCandidateImportStatus?
    /// Routes the running import's progress to the leaf line that draws it.
    let candidateKey: String
    /// Whether core allows cancelling the running import, which it does until
    /// the release is being written.
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

    /// The imported release's cloud upload; `nil` when the outbox has nothing
    /// queued for it.
    private var uploadObservation: UploadObservation? {
        guard case .complete(let releaseId, albumId: _) = importStatus else {
            return nil
        }
        return outboxStore.persistedUploadObservation(forRelease: releaseId)
    }

    /// The running import's cancel, or a disabled "Finishing" once the release
    /// is being written and can no longer be cancelled.
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
                    .themeText(.body)
                    .frame(width: 200)
                }
                else {
                    Label("Imported", systemImage: "checkmark.circle.fill")
                        .foregroundStyle(Theme.success)
                        .themeText(.body)
                }
                if let albumId = completedAlbumId {
                    Button("View in Library") { onViewInLibrary(albumId) }
                        .buttonStyle(.link)
                        .themeText(.body)
                }
            }
        }
        else if let status = importStatus {
            switch status {
            case .importing:
                // The same progress line the candidate's row draws, from the
                // same signal, so the two cannot disagree.
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
        // The progress line reads through the candidate reader, so the
        // preview mounts it as the app does.
        .candidateReaderPreviewEnvironment()
        .windowBackground()
    }
#endif
