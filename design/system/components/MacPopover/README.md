# MacPopover

A popover hanging off what it describes, its arrow pointing up at the control that opened it: the toolbar's Activity button, a library row's More button. It holds something to look at with nothing to decide, and closes on a click outside, with no Done. Every Mac popover draws this one face: the raised colour, a card's corners, a hairline edge and a shadow. Place it with `top` and `right` inside its window to hang it below the control, its arrow on its upper edge; place it with `bottom` and `right` to open it above the control, its arrow on its lower edge pointing down, as the Play on popover opens. `arrow` sets the arrow's middle from the right edge, 40 unless given. `flush` drops the padding, for content that pads itself.

**In the code:** macOS: `MacPopover` (BaeKit `Views/MacPopover.swift`), which `.macPopover` presents in place of SwiftUI's `.popover`.
