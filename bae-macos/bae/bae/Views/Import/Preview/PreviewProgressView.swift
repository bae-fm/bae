import BaeKit
import Combine
import SwiftUI

extension EnvironmentValues {
    @Entry
    var previewProgressPublisher: AnyPublisher<PlaybackPositionEvent, Never> =
        Empty().eraseToAnyPublisher()
}

struct PreviewProgressView: View {
    @Environment(\.previewProgressPublisher)
    private var positions
    let onSeek: (Double) -> Void

    var body: some View {
        SeekBarRepresentable(
            positions: positions,
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
                \.previewProgressPublisher,
                Just(
                    PlaybackPositionEvent.position(
                        progress: 0.4,
                        positionMs: 78_000,
                        durationMs: 195_000
                    )
                )
                .eraseToAnyPublisher()
            )
            .frame(width: 320, height: 40)
            .padding()
    }
#endif
