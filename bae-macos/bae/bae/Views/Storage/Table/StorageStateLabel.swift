import BaeKit
import SwiftUI

/// The storage badge: an in-flight transfer or queued upload wins over the
/// resting storage state.
struct StorageStateLabel: View {
    let release: ReleaseSummary
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        if let transfer = release.transfer {
            Label(transfer.label, systemImage: "arrow.down.circle")
                .foregroundStyle(.blue)
                .lineLimit(1)
        }
        else if let observation = outboxStore.storageUploadObservation(
            forRelease: release.id
        ) {
            switch observation {
            case .active(let progress):
                UploadActivityLabel(progress: progress)
            case .queueing, .awaiting:
                Label(
                    observation.transitionStatusText,
                    systemImage: "icloud.and.arrow.up"
                )
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }
        }
        else {
            switch release.storageState {
            case .local:
                Label("Local", systemImage: "folder").lineLimit(1)
            case .remote:
                if release.pinned {
                    Label("Pinned", systemImage: "pin.fill").lineLimit(1)
                }
                else {
                    Label("Cloud", systemImage: "cloud").lineLimit(1)
                }
            }
        }
    }
}

#if DEBUG
    #Preview("Storage states") {
        VStack(alignment: .leading, spacing: 10) {
            // Resting states: local, cloud, pinned.
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "r-local",
                    storageState: .local
                )
            )
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "r-cloud",
                    storageState: .remote,
                    pinned: false
                )
            )
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "r-pinned",
                    storageState: .remote,
                    pinned: true
                )
            )
            // An in-flight transition wins over the resting state.
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "r-transfer",
                    storageState: .remote,
                    transfer: .pin
                )
            )
            // A queued upload (id present in the injected outbox snapshot)
            // wins over the resting state.
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "rel-up-1",
                    storageState: .remote
                )
            )
        }
        .padding()
        .environment(PreviewData.outboxStore())
    }
#endif
