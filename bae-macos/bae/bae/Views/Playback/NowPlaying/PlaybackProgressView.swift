import BaeKit
import SwiftUI

struct PlaybackProgressView: View {
    /// Where the playing track is; `nil` while no track plays.
    let position: LatestSource<BridgeTrackPosition>?
    let showRemainingTime: Bool
    let onSeek: (Double) -> Void
    let onToggleRemainingTime: () -> Void

    var body: some View {
        SeekBarRepresentable(
            source: position.map(SeekBarSource.track),
            showRemainingTime: showRemainingTime,
            onSeek: onSeek,
            onToggleRemainingTime: onToggleRemainingTime
        )
    }
}
