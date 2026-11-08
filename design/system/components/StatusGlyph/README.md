# StatusGlyph

A tone's glyph in the tone's colour, a filled circle with its mark: info `i`, success a check, warning and danger `!` (told apart by colour). Neutral, accent and activity have none. `set="mac"` draws the macOS stand-in; without it, Android's Material Symbol, filled. `Notice`, the danger chips and the failed states use it.

**In the code:** macOS: BaeKit's `StatusTone.symbol` (`info.circle.fill`, `checkmark.circle.fill`, `exclamationmark.circle.fill`). Android: Status.kt's `StatusTone.icon` (`Info`, `CheckCircle`, `Error`).
