# Notice

A tinted box in a tone: info, success, warning, danger or neutral. It leads with the tone's glyph, as both apps do; neutral has none.

| Prop | |
| --- | --- |
| `tone` | `info`, `success`, `warning`, `danger` or `neutral` (the default). |
| `icon` | On unless `false`. Off where the content already carries the mark, such as a `MacErrorDisclosure` with its own icon. |
| `set` | `mac` draws macOS's glyph (a stand-in for the SF Symbol), top-aligned. Without it, Android's Material Symbol, centred. |

On the tint, grey text and grey button fills are the primary text made see-through, so they take the tint's colour.

The glyph is `StatusGlyph`: a filled circle with the tone's mark, about 120% of the text beside it: a 15.5px circle by 13px text on macOS (the 13pt SF Symbol), a 20dp Material icon by 14sp text on Android.

**In the code:** macOS: BaeKit's `.notice(_:)`, with the caller drawing `StatusTone.symbol`. Android: `Notice` (`ui/components/Status.kt`), which draws `StatusTone.icon`. Both change warning and danger to the filled circle with `!`.
