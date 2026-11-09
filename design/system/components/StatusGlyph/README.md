# StatusGlyph

A tone's glyph in the tone's colour, a filled circle with its mark: info `i`, success a check, warning and danger `!` (told apart by colour). Its colour is the tone's icon token (`danger-icon`, `warning-icon`, `info-icon`, `success-icon`), worked out from the tone: in light mode the tone made lighter and more saturated, since a filled shape needs less darkness than text; in dark mode the tone itself, danger mixed 65% with secondary grey so it stays calmer than the warning's amber. `outline` draws the outline form, a ring with the mark inside, for a quiet notice. `muted` draws it in grey (`text-tertiary`), for a failed view's centred notice, where it says only that something went wrong. Neutral, accent and activity have none. `set="mac"` draws the macOS stand-in; without it, Android's Material Symbol, filled. `MacNotice`, `AndroidNotice`, the danger chips and the failed states use it.

**In the code:** macOS: BaeKit's `StatusTone.symbol` (`info.circle.fill`, `checkmark.circle.fill`, `exclamationmark.circle.fill`; outline: the same without `.fill`). Android: Status.kt's `StatusTone.icon` (`Info`, `CheckCircle`, `Error`; outline: the `Outlined` set).

`size` on macOS is the circle's width, as the SF Symbol draws it; on Android it is the Material icon's box, whose circle fills 20 of its 24.
