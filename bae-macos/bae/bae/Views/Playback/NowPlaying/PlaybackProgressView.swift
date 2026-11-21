import BaeKit
import SwiftUI

struct PlaybackProgressView: View {
    /// Where the playing track is; `nil` while no track plays.
    let position: LatestSource<BridgeTrackPosition>?
    /// The parts of the same track whose bytes are here; `nil` while no
    /// track plays.
    let buffered: LatestSource<BridgeTrackBuffered>?
    let showRemainingTime: Bool
    let onSeek: (Double) -> Void
    let onToggleRemainingTime: () -> Void

    var body: some View {
        MacSeekBar(
            source: source,
            showRemainingTime: showRemainingTime,
            showsIdleClock: true,
            onSeek: onSeek,
            onToggleRemainingTime: onToggleRemainingTime
        )
    }

    private var source: SeekBarSource? {
        guard let position, let buffered else { return nil }
        return .track(position, buffered: buffered)
    }
}
