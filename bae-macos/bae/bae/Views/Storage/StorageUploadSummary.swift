import BaeKit
import SwiftUI

/// Release-wide upload progress and controls above the shared file list.
struct StorageUploadSummary: View {
    let group: BridgeUploadReleaseGroup
    let onCancel: () -> Void
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        let transfer = outboxStore.transfer(forRelease: group.releaseId)
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            HStack {
                UploadActivityLabel(fileCountOf: group.progress)
                Spacer()
                Text(
                    StorageUploadObservation.active(
                        progress: group.progress,
                        transfer: transfer
                    )
                    .throughputText ?? ""
                )
                .monospacedDigit()
                Button("Cancel", role: .destructive, action: onCancel)
                    .controlSize(.small)
            }
            Text(transfer?.bar?.text ?? "")
                .foregroundStyle(.secondary)
                .monospacedDigit()
            ProgressTrackBar(progress: transfer?.bar?.fraction ?? 0)
                .opacity(transfer?.bar == nil ? 0 : 1)
        }
        .themeText(.detail)
        .padding(.horizontal)
        .padding(.vertical, ThemeSpace.related)
    }
}
