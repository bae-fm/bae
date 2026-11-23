import BaeKit
import SwiftUI

/// The sheep on the Import screen while it has no folders. Every few seconds
/// it plays one moment from `SheepSequence`, then rests: it gobbles a line of
/// releases from the left or the right, catches one falling from above,
/// breathes one in, bops to its music, or rolls on the spot. The loop and its
/// moments are the design's `ImportSheep`
/// (`design/system/components/ImportSheep/README.md`).
///
/// `size` is the sheep's width; the drawing's box is wider and taller, to
/// leave room for the releases on their way in. `still` and Reduce Motion
/// show the sheep at rest with a cover beside it.
struct ImportSheep: View {
    let size: CGFloat
    var still = false

    @Environment(\.accessibilityReduceMotion)
    private var reduceMotion
    @Environment(\.clockNow)
    private var fixedNow
    @State
    private var started = Date()

    var body: some View {
        let motion = Motion(
            still: still,
            reduceMotion: reduceMotion,
            fixedNow: fixedNow
        )
        TimelineView(.animation(paused: motion.isPaused)) { timeline in
            let painter = motion.painter(
                elapsed: timeline.date.timeIntervalSince(started)
            )
            Canvas { context, canvasSize in
                var context = context
                painter.draw(into: &context, size: canvasSize)
            }
        }
        .frame(
            width: (size * ImportSheepPainter.viewBox.width / SheepArt.box.width)
                .rounded(),
            height: (size * ImportSheepPainter.viewBox.height
                / SheepArt.box.width)
                .rounded()
        )
        .accessibilityHidden(true)
    }

    enum Motion {
        case playing
        case firstFrame
        case still

        init(still: Bool, reduceMotion: Bool, fixedNow: Date?) {
            if still {
                self = .still
            }
            else if fixedNow != nil {
                // The fixed clock holds the moving pose at the board's first
                // frame regardless of the capture Mac's Reduce Motion setting.
                self = .firstFrame
            }
            else {
                self = reduceMotion ? .still : .playing
            }
        }

        var isPaused: Bool { self != .playing }

        func painter(elapsed: TimeInterval) -> ImportSheepPainter {
            if self == .still {
                return ImportSheepPainter(moment: SheepSequence.still, time: 0)
            }
            let playing = SheepSequence.moment(at: isPaused ? 0 : elapsed)
            return ImportSheepPainter(
                moment: playing.moment,
                time: playing.time
            )
        }
    }
}
