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

    /// Outbox progress for this release, if any work is in flight (releases
    /// with nothing left to ship are absent from the per-release map). Drives
    /// the "uploading…" indicator and suppresses transfer actions — acting
    /// mid-upload races the observer that completes the unmanaged → managed
    /// step.
    private var uploadProgress: BridgeUploadProgress? {
        outboxStore.progress(forRelease: release.summary.id)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            storageStatus
            // Read the live transfer state off the identity-stable summary so a
            // running pin/unpin/manage/unmanage updates the bar in place.
            if let transfer = release.summary.transfer {
                progressBar(
                    label: transfer.label
                )
            }
            else if let progress = uploadProgress {
                progressBar(
                    value: progress.fraction,
                    label: String(
                        localized:
                            "Uploading (\(Int(progress.pending)) remaining)…"
                    )
                )
            }
            else {
                transferActions
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
    }

    private func progressBar(value: Double? = nil, label: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            ProgressView(value: value)
                .progressViewStyle(.linear)
            Text(label)
                .font(.callout)
                .foregroundStyle(.secondary)
        }
    }

    private var storageStatus: some View {
        HStack(spacing: 6) {
            switch release.summary.storageState {
            case .local:
                Image(systemName: "folder")
                Text("Unmanaged")
            case .remote:
                if release.summary.pinned {
                    Image(systemName: "pin.fill")
                    Text("Pinned for offline")
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

    // Unmanaged (local): the only offered transition is uploading to the cloud.
    #Preview("Storage Band — Local") {
        previewStorageBand(
            storageState: .local,
            pinned: false,
            storageActions: [.makeRemote]
        )
        .preferredColorScheme(.dark)
    }

    // In the cloud, not kept offline: can pin or pull back local.
    #Preview("Storage Band — Cloud") {
        previewStorageBand(
            storageState: .remote,
            pinned: false,
            storageActions: [.pin, .makeLocal]
        )
        .preferredColorScheme(.dark)
    }

    // In the cloud, pinned for offline: can unpin or pull back local.
    #Preview("Storage Band — Pinned") {
        previewStorageBand(
            storageState: .remote,
            pinned: true,
            storageActions: [.unpin, .makeLocal]
        )
        .preferredColorScheme(.dark)
    }
#endif
