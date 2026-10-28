import BaeKit
import SwiftUI

/// A release's storage state, transfer or upload progress, and storage
/// actions. An upload's bar and bytes are native views that set themselves
/// from the release's stream, so moving bytes run no SwiftUI update.
struct StorageStatusBand: View {
    let release: ReleaseDetail
    let onAction: (BridgeReleaseStorageAction) -> Void
    let onExport: () -> Void
    let onSaveAs: () -> Void
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            // A cloud transition in flight is the release's storage state
            // while it runs: its phase, bar, and count are one line.
            if let upload = release.summary.upload {
                if upload.progress.uploading > 0 {
                    uploadLine(upload.progress)
                        .themeText(.body)
                }
                else {
                    // Files only waiting: nothing is moving to measure.
                    UploadActivityLabel(phaseOf: upload.progress)
                        .themeText(.body)
                }
            }
            else {
                storageStatus
            }
            // Read off the identity-stable summary so the bar updates in place.
            if let transfer = release.summary.transfer {
                ProgressLine(transfer.label, progress: nil)
                    .themeText(.body)
            }
            else if !release.summary.transitioning {
                transferActions
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
    }

    /// The phase, then a bar and the bytes the release holds of its size,
    /// both set from the release's upload stream.
    private func uploadLine(_ progress: BridgeUploadProgress) -> some View {
        let source = outboxStore.releaseTransfer(forRelease: release.summary.id)
        return HStack(spacing: ThemeSpace.related) {
            UploadActivityLabel(phaseOf: progress)
                .lineLimit(1)
                .foregroundStyle(.secondary)
                .truncationMode(.middle)
                .layoutPriority(1)
            StreamBar(stream: .upload(source))
                .frame(minWidth: 48)
            StreamLabel(
                stream: .uploadBytes(
                    outboxStore.uploadProgress(forRelease: release.summary.id)
                )
            )
        }
    }

    /// The resting storage state.
    private var storageStatus: some View {
        HStack(spacing: ThemeSpace.inline) {
            switch release.summary.storageState {
            case .local:
                Image(systemName: "folder")
                Text("Local")
            case .remote:
                if release.summary.pinned {
                    Image(systemName: "pin.fill")
                    Text("Pinned")
                }
                else {
                    Image(systemName: "cloud")
                    Text("Cloud")
                }
            }
        }
        .themeText(.body)
        .foregroundStyle(.secondary)
    }

    private var transferActions: some View {
        Group {
            ForEach(release.storageActions, id: \.self) { action in
                Button(action: { onAction(action) }) {
                    Label(action.label, systemImage: action.systemImage)
                }
            }
            // Export and Save As change no state, so every release offers them.
            Button(action: { onExport() }) {
                Label("Export…", systemImage: "square.and.arrow.up")
            }
            Button(action: { onSaveAs() }) {
                Label("Save As…", systemImage: "square.and.arrow.down")
            }
        }
    }
}

#if DEBUG
    @MainActor
    private func previewStorageBand(
        storageState: BridgeReleaseStorageState,
        pinned: Bool,
        storageActions: [BridgeReleaseStorageAction]
    ) -> some View {
        StorageStatusBand(
            release: PreviewData.storageRelease(
                storageState: storageState,
                pinned: pinned,
                storageActions: storageActions
            ),
            onAction: { _ in },
            onExport: {},
            onSaveAs: {}
        )
        .frame(width: 560)
        .background(Theme.background)
        .environment(OutboxStore(uploadsPaused: false))
    }

    // Local: the only offered transition is uploading to the cloud.
    #Preview("Storage Band — Local") {
        previewStorageBand(
            storageState: .local,
            pinned: false,
            storageActions: [.upload]
        )
        .preferredColorScheme(.dark)
    }

    // In the cloud, not pinned: can pin or pull back local.
    #Preview("Storage Band — Cloud") {
        previewStorageBand(
            storageState: .remote,
            pinned: false,
            storageActions: [.pin, .detachAudio]
        )
        .preferredColorScheme(.dark)
    }

    // In the cloud and pinned: can unpin or pull back local.
    #Preview("Storage Band — Pinned") {
        previewStorageBand(
            storageState: .remote,
            pinned: true,
            storageActions: [.unpin, .detachAudio]
        )
        .preferredColorScheme(.dark)
    }
#endif
