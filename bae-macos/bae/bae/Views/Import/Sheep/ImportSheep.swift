import SwiftUI

/// The sheep on the Import screen while it has no folders. Every few seconds
/// it plays one moment from `SheepSequence`, then rests: it gobbles a line of
/// releases from the left or the right, catches one falling from above,
/// breathes one in, bops to its music, or rolls on the spot. The loop and its
/// moments are the design's `ImportSheep`
/// (`design/system/components/ImportSheep/README.md`).
///
/// `size` is the sheep's width; the drawing's box is wider and taller, to
/// leave room for the releases on their way in. When the person has asked for
/// reduced motion it holds still.
struct ImportSheep: View {
    let size: CGFloat

    @Environment(\.accessibilityReduceMotion)
    private var reduceMotion
    @State
    private var started = Date()

    var body: some View {
        TimelineView(.animation(paused: reduceMotion)) { timeline in
            let playing = SheepSequence.moment(
                at: timeline.date.timeIntervalSince(started)
            )
            let painter =
                reduceMotion
                ? ImportSheepPainter(moment: SheepSequence.still, time: 0)
                : ImportSheepPainter(
                    moment: playing.moment,
                    time: playing.time
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
}

#if DEBUG
    #Preview("Import sheep") {
        ImportSheep(size: 140)
            .padding(ThemeSpace.page)
            .windowBackground()
    }
#endif
