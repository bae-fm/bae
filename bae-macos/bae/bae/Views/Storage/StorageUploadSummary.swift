import BaeKit
import SwiftUI

/// Release-wide upload progress and controls above the shared file list.
struct StorageUploadSummary: View {
    let group: BridgeUploadReleaseGroup
    let onCancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
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
                    .opacity(group.progress.canCancel ? 1 : 0)
                    .allowsHitTesting(group.progress.canCancel)
                    .disabled(!group.progress.canCancel)
                    .accessibilityHidden(!group.progress.canCancel)
            }
            Text(group.progress.bar?.text ?? "")
                .foregroundStyle(.secondary)
                .monospacedDigit()
            ProgressTrackBar(progress: group.progress.bar?.fraction ?? 0)
                .opacity(group.progress.bar == nil ? 0 : 1)
            ForEach(group.progress.sourceUnavailablePaths, id: \.self) { path in
                Text(path)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .help(path)
            }
        }
        .font(.caption)
        .padding(.horizontal)
        .padding(.vertical, 8)
    }
}
