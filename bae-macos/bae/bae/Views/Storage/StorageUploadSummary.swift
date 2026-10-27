import BaeKit
import SwiftUI

/// Release-wide upload progress and controls above the shared file list.
struct StorageUploadSummary: View {
    let upload: BridgeReleaseUpload
    /// How fast the release's files go; `nil` while no rate has arrived for
    /// it.
    let throughputBps: UInt64?
    let onCancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            HStack {
                UploadActivityLabel(fileCountOf: upload.progress)
                Spacer()
                Text(
                    StorageUploadObservation(
                        upload: upload,
                        throughputBps: throughputBps
                    )
                    .throughputText ?? ""
                )
                .monospacedDigit()
                Button("Cancel", role: .destructive, action: onCancel)
                    .controlSize(.small)
            }
            Text(upload.bar?.text ?? "")
                .foregroundStyle(.secondary)
                .monospacedDigit()
            ProgressTrackBar(progress: upload.bar?.fraction ?? 0)
                .opacity(upload.bar == nil ? 0 : 1)
        }
        .themeText(.detail)
        .padding(.horizontal)
        .padding(.vertical, ThemeSpace.related)
    }
}
