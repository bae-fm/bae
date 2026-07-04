import SwiftUI

struct DownloadTransferProgressView: View {
    let progress: BridgeDownloadTransferProgress

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ProgressView(value: progress.fraction)
                .progressViewStyle(.linear)
            Text(progress.bytesText)
                .font(.caption2.monospacedDigit())
                .foregroundStyle(.secondary)
        }
    }
}
