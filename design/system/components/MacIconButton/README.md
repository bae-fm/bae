# MacIconButton

A 28pt glyph button: secondary at rest, a hover fill. `active` for a mode that is on (shuffle, repeat, the queue shown, casting). `faint` draws it in tertiary text, for a button that should sit back (a notice's ×). `small` makes it 20pt, for a control inside a line of text, such as a transfer's Pause in the Activity panel. `boxed` gives it the push button's fill at rest, so it reads as a button among text.

**In the code:** macOS: a `Button` with `IconHoverButtonStyle` (`Views/Components/IconHoverButtonStyle.swift`).
