import BaeKit
import SwiftUI

extension EnvironmentValues {
    /// Where the import tab's preview is in its clip, for the native bar
    /// that reads it each display frame; `nil` where no preview plays.
    @Entry
    var previewPosition: LatestSource<BridgePreviewPosition>?
}

struct PreviewProgressView: View {
    @Environment(\.previewPosition)
    private var position
    let onSeek: (Double) -> Void

    var body: some View {
        SeekBarRepresentable(
            source: position.map(SeekBarSource.preview),
            showRemainingTime: false,
            onSeek: onSeek,
            onToggleRemainingTime: nil
        )
    }
}

#if DEBUG
    #Preview("Preview progress bar") {
        PreviewProgressView(onSeek: { _ in })
            .environment(
                \.previewPosition,
                PreviewData.previewPosition(
                    positionMs: 78_000,
                    durationMs: 195_000
                )
            )
            .frame(width: 320, height: 40)
            .padding()
    }
#endif
