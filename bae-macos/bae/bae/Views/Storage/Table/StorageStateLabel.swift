import BaeKit
import SwiftUI

/// The storage state chip: an in-flight transfer or queued upload wins over
/// the resting storage state.
struct StorageStateLabel: View {
    let release: ReleaseSummary
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        if let transfer = release.transfer {
            StatusChip(
                verbatim: transfer.label,
                tone: .activity,
                symbol: "arrow.down.circle"
            )
        }
        else if let observation = outboxStore.storageUploadObservation(
            forRelease: release.id
        ) {
            switch observation {
            case .active(let progress, _):
                ProgressLine(
                    progress: observation.progressBar.fraction,
                    detail: observation.throughputText
                ) {
                    UploadActivityLabel(progress: progress)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(observation.transitionStatusText)
                .accessibilityValue(observation.throughputText ?? "")
            case .queueing, .awaiting:
                StatusChip(
                    verbatim: observation.transitionStatusText,
                    symbol: "icloud.and.arrow.up"
                )
            }
        }
        else {
            switch release.storageState {
            case .local:
                StatusChip("Local", symbol: "folder")
            case .remote:
                if release.pinned {
                    StatusChip("Pinned", symbol: "pin.fill")
                }
                else {
                    StatusChip("Cloud", symbol: "cloud")
                }
            }
        }
    }
}

#if DEBUG
    #Preview("Storage states") {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
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
            // A queued upload in the preview outbox wins over the resting
            // state.
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
