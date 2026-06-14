import AppKit

/// NSSlider subclass that reports drag start/end and jumps to the clicked
/// position. `mouseDown` blocks until the user releases, so `isDragging` is
/// accurate. Shared by the playback and preview progress NSViews.
class SeekSlider: NSSlider {
    var onSeekComplete: ((Double) -> Void)?
    private(set) var isDragging = false

    override func mouseDown(with event: NSEvent) {
        // NSSlider's default click-on-track is a page-step increment toward
        // the click, not a jump-to-click. That moves doubleValue only a tiny
        // amount, so the resulting seek gets rejected as a same-position
        // seek (< 100ms diff). Compute the click position synchronously so
        // click and drag both commit to the clicked location.
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
}
