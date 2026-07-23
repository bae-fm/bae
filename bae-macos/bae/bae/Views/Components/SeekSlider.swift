import AppKit
import BaeKit

/// NSSlider subclass that reports drag start/end and jumps to the clicked
/// position. `mouseDown` blocks until the user releases, so `isDragging` is
/// accurate. Used by `SeekBarNSView`.
class SeekSlider: NSSlider {
    var onSeekComplete: ((Double) -> Void)?
    private(set) var isDragging = false

    override class var cellClass: AnyClass? {
        get { SlimSeekSliderCell.self }
        set {}
    }

    override func mouseDown(with event: NSEvent) {
        // NSSlider's default click-on-track is a page-step increment toward
        // the click, not a jump-to-click. That moves doubleValue only a tiny
        // amount, so core treats the result as a same-position seek. Compute
        // the click position synchronously so click and drag both commit to
        // the clicked location.
        if let cell = cell as? NSSliderCell {
            let point = convert(event.locationInWindow, from: nil)
            let knobRect = cell.knobRect(flipped: isFlipped)
            if !knobRect.contains(point) {
                let barRect = cell.barRect(flipped: isFlipped)
                if barRect.width > 0 {
                    let ratio = Double((point.x - barRect.minX) / barRect.width)
                    let value = ratio * (maxValue - minValue) + minValue
                    doubleValue = max(minValue, min(maxValue, value))
                }
            }
        }

        isDragging = true
        super.mouseDown(with: event)
        isDragging = false
        onSeekComplete?(doubleValue)
    }

    /// This slider's current value mapped to a position within `durationMs`,
    /// clamped to the track. Drives the live seek-preview label during a drag;
    /// the actual seek sends the raw ratio to core via `onSeekComplete`.
    func positionMs(forDuration durationMs: UInt64) -> UInt64 {
        UInt64(max(0, min(1, doubleValue)) * Double(durationMs))
    }
}

/// Draws the slim seek track through the shared `ProgressTrackDrawing`, so the
/// interactive seek bar and every passive bar in the app share one look, and
/// no knob. `SeekSlider.mouseDown` still relies on this cell's default
/// `knobRect`/`barRect` geometry for click-to-seek math.
final class SlimSeekSliderCell: NSSliderCell {
    private let trackHeight: CGFloat = 5

    override func drawBar(inside rect: NSRect, flipped _: Bool) {
        var bar = rect
        bar.origin.y = rect.midY - trackHeight / 2
        bar.size.height = trackHeight

        let span = maxValue - minValue
        let fraction =
            span > 0 ? min(max((doubleValue - minValue) / span, 0), 1) : 0
        ProgressTrackDrawing.draw(in: bar, fraction: fraction)
    }

    override func drawKnob(_: NSRect) {
        // No visible knob — the slim track shows position by its fill.
    }
}
