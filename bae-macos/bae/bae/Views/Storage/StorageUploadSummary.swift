import BaeKit
import SwiftUI

/// Release-wide upload progress and controls above the shared file list.
struct StorageUploadSummary: View {
    let group: BridgeUploadReleaseGroup
    let onCancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            HStack {
                UploadActivityLabel(progress: group.progress)
                Spacer()
                Text(
                    StorageUploadObservation.active(
                        progress: group.progress,
                        throughputBps: group.throughputBps
                    )
                    .throughputText ?? ""
                )
                .monospacedDigit()
                Button("Cancel", role: .destructive, action: onCancel)
                    .controlSize(.small)
            }
            Text(group.progress.bar?.text ?? "")
                .foregroundStyle(.secondary)
                .monospacedDigit()
            ProgressTrackBar(progress: group.progress.bar?.fraction ?? 0)
                .opacity(group.progress.bar == nil ? 0 : 1)
        }
        .themeText(.detail)
        .padding(.horizontal)
        .padding(.vertical, ThemeSpace.related)
    }
}
