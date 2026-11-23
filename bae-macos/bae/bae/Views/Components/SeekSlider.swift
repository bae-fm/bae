import AppKit
import BaeKit
import OSLog

private let seekSliderLogger = Logger.bae("SeekSlider")

/// NSSlider subclass that reports drag start/end and jumps to the clicked
/// position. `mouseDown` blocks until the user releases, so `isDragging` is
/// accurate. Used by `SeekBarNSView`. The slider's value runs from 0 to 1:
/// core sends the played fraction, or the person drags to a new one. The cell
/// draws that value directly as the fill fraction.
class SeekSlider: NSSlider {
    var accent: NSColor = .controlAccentColor {
        didSet { needsDisplay = true }
    }

    /// The parts of the track whose bytes are here, drawn lighter under the
    /// played part.
    var held: [BridgeTrackSpan] = [] {
        didSet {
            if held != oldValue { needsDisplay = true }
        }
    }

    var onSeekComplete: ((Double) -> Void)?
    private(set) var isDragging = false

    override class var cellClass: AnyClass? {
        get { SlimSeekSliderCell.self }
        set {}
    }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        minValue = 0
        maxValue = 1
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    override func mouseDown(with event: NSEvent) {
        // Disabled while the track's length is unknown: there is no position for
        // a click to seek to.
        guard isEnabled else { return }
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
                    // A click past either end of the bar seeks to that end.
                    let ratio = Double((point.x - barRect.minX) / barRect.width)
                    doubleValue = max(0, min(1, ratio))
                }
            }
        }

        isDragging = true
        super.mouseDown(with: event)
        isDragging = false
        onSeekComplete?(doubleValue)
    }

    /// The slider's frame is taller than the track so a click near the track
    /// seeks; its accessibility frame is the track the board draws.
    override func accessibilityFrame() -> NSRect {
        guard let cell = cell as? SlimSeekSliderCell, let window else {
            return super.accessibilityFrame()
        }
        let track = cell.trackRect(in: bounds, flipped: isFlipped)
        return window.convertToScreen(convert(track, to: nil))
    }

    /// Screen readers read the position as a whole percent, 0 to 100, as a
    /// player's seek slider says it.
    override func accessibilityValue() -> Any? {
        Int(AccessibilityPercentage.percent(for: doubleValue))
    }

    override func accessibilityMaxValue() -> Any? {
        AccessibilityPercentage.range.upperBound
    }

    override func setAccessibilityValue(_ value: Any?) {
        guard isEnabled else {
            seekSliderLogger.debug("Ignored a set of the disabled slider")
            return
        }
        guard let percent = (value as? NSNumber)?.doubleValue else {
            seekSliderLogger.error(
                "Ignored a position that is no number: \(String(describing: value))"
            )
            return
        }
        doubleValue = AccessibilityPercentage.fraction(for: percent)
        onSeekComplete?(doubleValue)
    }

    /// This slider's current value mapped to a position within `durationMs`,
    /// clamped to the track. Drives the live seek-shot label during a drag;
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

    /// The drawn track, centred in `rect`.
    func trackRect(in rect: NSRect, flipped: Bool) -> NSRect {
        var bar = barRect(flipped: flipped)
        bar = NSRect(
            x: bar.minX,
            y: rect.midY - trackHeight / 2,
            width: bar.width,
            height: trackHeight
        )
        return bar
    }

    override func drawBar(inside rect: NSRect, flipped: Bool) {
        let bar = trackRect(in: rect, flipped: flipped)

        guard let slider = controlView as? SeekSlider else {
            preconditionFailure("SlimSeekSliderCell belongs to SeekSlider")
        }
        ProgressTrackDrawing.drawTrack(in: bar)
        ProgressTrackDrawing.drawSpans(
            slider.held,
            in: bar,
            color: slider.accent.withAlphaComponent(
                CGFloat(ThemeOpacity.tintStrong)
            )
        )
        ProgressTrackDrawing.drawFill(
            in: bar,
            fraction: doubleValue,
            accent: slider.accent
        )
    }

    override func drawKnob(_: NSRect) {
        // No visible knob — the slim track shows position by its fill.
    }
}
