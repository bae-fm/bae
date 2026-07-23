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

/// Draws the slim seek/progress track: a 5pt rounded groove in a neutral
/// low-opacity fill, its played portion filled with the accent gradient, and
/// no knob. `SeekSlider.mouseDown` still relies on this cell's default
/// `knobRect`/`barRect` geometry for click-to-seek math.
final class SlimSeekSliderCell: NSSliderCell {
    private let trackHeight: CGFloat = 5

    override func drawBar(inside rect: NSRect, flipped _: Bool) {
        var bar = rect
        bar.origin.y = rect.midY - trackHeight / 2
        bar.size.height = trackHeight
        let radius = trackHeight / 2

        NSColor.white.withAlphaComponent(0.12).setFill()
        NSBezierPath(roundedRect: bar, xRadius: radius, yRadius: radius).fill()

        let span = maxValue - minValue
        guard span > 0 else { return }
        let fraction = max(0, min(1, CGFloat((doubleValue - minValue) / span)))
        guard fraction > 0 else { return }

        var fill = bar
        fill.size.width = bar.width * fraction
        let fillPath = NSBezierPath(
            roundedRect: fill,
            xRadius: radius,
            yRadius: radius
        )

        let base = NSColor(Theme.accent)
        guard let lighter = base.blended(withFraction: 0.3, of: .white),
            let gradient = NSGradient(starting: base, ending: lighter)
        else {
            assertionFailure("accent gradient could not be built")
            base.setFill()
            fillPath.fill()
            return
        }
        gradient.draw(in: fillPath, angle: 0)
    }

    override func drawKnob(_: NSRect) {
        // No visible knob — the slim track shows position by its fill.
    }
}
