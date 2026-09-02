import BaeKit
import SwiftUI

/// Storage state, any in-flight transfer/upload progress, and the available
/// storage actions for a release. A separate leaf so transfer and outbox ticks
/// re-render only this band, not the sheet's file table.
struct StorageStatusBand: View {
    let release: ReleaseDetail
    let onAction: (BridgeReleaseStorageAction) -> Void
    let onExport: () -> Void
    let onSaveAs: () -> Void
    @Environment(OutboxStore.self)
    private var outboxStore

    /// Outbox progress for this release while its make-Remote transition is
    /// unfinished, including publication after all provider bytes land. Drives
    /// the "uploading…" indicator and suppresses transfer actions — acting
    /// mid-upload races the observer that completes the local → cloud
    /// step.
    private var uploadObservation: StorageUploadObservation? {
        outboxStore.storageUploadObservation(forRelease: release.summary.id)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            // A cloud transition in flight is the release's storage state
            // while it runs: its phase, bar, and count are one line.
            if let observation = uploadObservation {
                ProgressLine(
                    progress: observation.progressBar.fraction,
                    detail: observation.progressDetailText
                ) {
                    uploadLabel(observation)
                }
                .font(.callout)
            }
            else {
                storageStatus
            }
            // Read the live transfer state off the identity-stable summary so a
            // running pin/unpin/cloud/local transition updates the bar in place.
            if let transfer = release.summary.transfer {
                ProgressLine(transfer.label, progress: nil)
                    .font(.callout)
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

    /// Storage actions are available only at rest. Publication and
    /// cancellation can have no file bytes left, but they are still active
    /// cloud transitions and keep the action area occupied by their phase.
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
        HStack(spacing: 6) {
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
        .font(.callout)
        .foregroundStyle(.secondary)
    }

    private var transferActions: some View {
        Group {
            ForEach(release.storageActions, id: \.self) { action in
                Button(action: { onAction(action) }) {
                    Label(action.label, systemImage: action.systemImage)
                }
            }
            // Export (verbatim) and Save As (preset workup) are pure outputs —
            // no state change — so both are offered for every release regardless
            // of locality, not among the core-computed `storageActions`.
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
