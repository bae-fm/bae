import BaeKit
import SwiftUI

/// A release's storage state, transfer or upload progress, and storage
/// actions; a separate view so progress ticks re-render only this band.
struct StorageStatusBand: View {
    let release: ReleaseDetail
    let onAction: (BridgeReleaseStorageAction) -> Void
    let onExport: () -> Void
    let onSaveAs: () -> Void
    @Environment(OutboxStore.self)
    private var outboxStore

    /// This release's outbox progress while its move to the cloud is
    /// unfinished; transfer actions stay hidden meanwhile, since acting
    /// mid-upload races the observer that completes the move.
    private var uploadObservation: StorageUploadObservation? {
        outboxStore.storageUploadObservation(forRelease: release.summary.id)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            // A cloud transition in flight is the release's storage state
            // while it runs: its phase, bar, and count are one line.
            if let observation = uploadObservation {
                ProgressLine(
                    progress: observation.progressBar.fraction,
                    detail: observation.progressDetailText
                ) {
                    uploadLabel(observation)
                }
                .themeText(.body)
            }
            else {
                storageStatus
            }
            // Read off the identity-stable summary so the bar updates in place.
            if let transfer = release.summary.transfer {
                ProgressLine(transfer.label, progress: nil)
                    .themeText(.body)
            }
            else if Self.showsTransferActions(
                uploadObservation: uploadObservation
            ) {
                transferActions
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
    }

    /// Storage actions are offered only while no cloud transition runs.
    static func showsTransferActions(
        uploadObservation: StorageUploadObservation?
    ) -> Bool {
        uploadObservation == nil
    }

    @ViewBuilder
    private func uploadLabel(
        _ observation: StorageUploadObservation
    ) -> some View {
        switch observation {
        case .active(let progress, _):
            UploadActivityLabel(progress: progress)
        case .queueing, .awaiting:
            Label(
                observation.transitionPhaseText,
                systemImage: "icloud.and.arrow.up"
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
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
    }

    // Local: the only offered transition is uploading to the cloud.
    #Preview("Storage Band — Local") {
        previewStorageBand(
            storageState: .local,
            pinned: false,
            storageActions: [.makeRemote]
        )
        .preferredColorScheme(.dark)
    }

    // In the cloud, not pinned: can pin or pull back local.
    #Preview("Storage Band — Cloud") {
        previewStorageBand(
            storageState: .remote,
            pinned: false,
            storageActions: [.pin, .makeLocal]
        )
        .preferredColorScheme(.dark)
    }

    // In the cloud and pinned: can unpin or pull back local.
    #Preview("Storage Band — Pinned") {
        previewStorageBand(
            storageState: .remote,
            pinned: true,
            storageActions: [.unpin, .makeLocal]
        )
        .preferredColorScheme(.dark)
    }
#endif
