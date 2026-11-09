# Notice

A tinted box in a tone: info, success, warning, danger or neutral. It leads with the tone's glyph, as both apps do; neutral has none.

| Prop | |
| --- | --- |
| `tone` | `info`, `success`, `warning`, `danger` or `neutral` (the default). |
| `icon` | On unless `false`. Off where the content already carries the mark, such as a `MacErrorDisclosure` with its own icon. |

Info, success, warning and danger each have their own fill (`info-notice` …): pale in light, a solid light colour in dark, where a see-through tint over the dark background reads grey and a yellow one brown. Text on it is `on-notice` (dark in both themes), as is the glyph in dark and any accent text (`notice-accent`), such as a text-button Retry. Grey text and grey button fills are that text made see-through. Neutral keeps the neutral tint and the screen's own text.

Its text takes the size of the screen it sits in: 13 in a Mac window, 14 on Android. The glyph is `StatusGlyph`, a filled circle with the tone's mark, 1.19 times the text (15.5 on a Mac, 16.7 on Android), at the top left, centred on the first line. One drawing serves both: the SF Symbol and Material's filled circle look alike.

**In the code:** macOS: BaeKit's `.notice(_:)`, with the caller drawing `StatusTone.symbol`. Android: `Notice` (`ui/components/Status.kt`), which draws `StatusTone.icon`. Both change warning and danger to the filled circle with `!`.
