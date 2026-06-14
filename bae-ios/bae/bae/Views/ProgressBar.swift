import Combine
import SwiftUI

/// The seek slider, driven by the high-frequency position subject so only this
/// view re-renders on each tick. While dragging, follows the finger; the seek
/// commits on release. Shared by the compact `NowPlayingBar` and the
/// `ExpandedNowPlayingView` so both scrub through the one component.
struct ProgressBar: View {
    let positionSubject: CurrentValueSubject<PlaybackPositionEvent, Never>
    let onSeek: (Double) -> Void

    @State
    private var progress: Double = 0
    @State
    private var elapsed: String = ""
    @State
    private var remaining: String = ""
    @State
    private var dragRatio: Double?

    var body: some View {
        HStack(spacing: 8) {
            Text(elapsed)
                .font(.caption2.monospacedDigit())
                .foregroundStyle(.secondary)
            Slider(
                value: Binding(
                    get: { dragRatio ?? progress },
                    set: { dragRatio = $0 }
                ),
                in: 0...1,
                onEditingChanged: { editing in
                    if !editing, let ratio = dragRatio {
                        onSeek(ratio)
                        dragRatio = nil
                    }
                }
            )
            Text(remaining)
                .font(.caption2.monospacedDigit())
                .foregroundStyle(.secondary)
        }
        .onReceive(positionSubject) { event in
            switch event {
            case .position(let progress, let elapsed, let remaining):
                // Ignore ticks while the user is scrubbing so the thumb doesn't
                // snap back mid-drag.
                if dragRatio == nil {
                    self.progress = max(0, min(1, progress))
                }
                self.elapsed = elapsed
                self.remaining = remaining
            case .reset:
                progress = 0
                elapsed = ""
                remaining = ""
                dragRatio = nil
            }
        }
    }
}
