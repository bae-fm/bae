# StatusGlyph

A tone's glyph in the tone's colour, a ring with its mark inside, the one form every status glyph takes: info `i`, success a check, warning and danger `!` (told apart by colour). Its colour is the tone's icon token (`danger-icon`, `warning-icon`, `info-icon`, `success-icon`), worked out from the tone: in light mode the tone made lighter and more saturated, since a glyph needs less darkness than text; in dark mode the tone itself, danger mixed 65% with secondary grey so it stays calmer than the warning's amber. `inline` sizes it at 1.19 times the text beside it, which the notices use. `muted` draws it in grey (`text-tertiary`), for a failed view's centred notice, where it says only that something went wrong. Neutral, accent and activity have none. `set="mac"` draws the macOS stand-in; without it, Android's Material Symbol, outlined. `MacNotice`, `AndroidNotice`, the danger chips and the failed states use it.

**In the code:** macOS: BaeKit's `StatusTone.symbol` (`info.circle`, `checkmark.circle`, `exclamationmark.circle`). Android: Status.kt's `StatusTone.icon` (the `Outlined` set: `Info`, `CheckCircle`, `ErrorOutline`).

`size` on macOS is the circle's width, as the SF Symbol draws it; on Android it is the Material icon's box, whose circle fills 20 of its 24.
