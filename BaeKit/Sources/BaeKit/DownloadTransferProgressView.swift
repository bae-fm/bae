import SwiftUI

public struct DownloadTransferProgressView: View {
    public let progress: BridgeDownloadTransferProgress

    public init(progress: BridgeDownloadTransferProgress) {
        self.progress = progress
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ProgressView(value: progress.fraction)
                .progressViewStyle(.linear)
            Text(progress.bytesText)
                .themeText(.fine)
                .monospacedDigit()
                .foregroundStyle(.secondary)
        }
    }
}
