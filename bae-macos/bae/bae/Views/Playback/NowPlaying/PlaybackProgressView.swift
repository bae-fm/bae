import BaeKit
import Combine
import SwiftUI

extension EnvironmentValues {
    @Entry
    var playbackPositionPublisher: AnyPublisher<PlaybackPositionEvent, Never> =
        Empty().eraseToAnyPublisher()
}

struct PlaybackProgressView: View {
    @Environment(\.playbackPositionPublisher)
    private var positions
    let showRemainingTime: Bool
    let onSeek: (Double) -> Void
    let onToggleRemainingTime: () -> Void

    var body: some View {
        SeekBarRepresentable(
            positions: positions,
            showRemainingTime: showRemainingTime,
            onSeek: onSeek,
            onToggleRemainingTime: onToggleRemainingTime
        )
    }
}
