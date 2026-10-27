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
        else if let observation = outboxStore.uploadObservation(of: release) {
            if let fraction = observation.progressFraction {
                ProgressLine(
                    progress: fraction,
                    detail: observation.throughputText
                ) {
                    UploadActivityLabel(phaseOf: observation.progress)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(observation.transitionStatusText)
                .accessibilityValue(observation.throughputText ?? "")
            }
            else {
                // Files only waiting: nothing is moving to measure.
                UploadActivityLabel(phaseOf: observation.progress)
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
            // Files waiting or being sent win over the resting state.
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: PreviewData.uploadGroup.releaseId,
                    storageState: .local,
                    upload: PreviewData.rowUploads[
                        PreviewData.uploadGroup.releaseId
                    ]
                )
            )
        }
        .padding()
        .environment(PreviewData.outboxStore())
    }
#endif
