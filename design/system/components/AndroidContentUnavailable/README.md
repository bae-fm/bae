# AndroidContentUnavailable

An empty or unavailable screen's message: one line of body text in the secondary colour, centred in what's left of the screen, with no glyph above it ("No albums yet", "No plays yet"). The children are its actions, in a row 12dp under it, as `MacContentUnavailable` spaces its own. `MacContentUnavailable` is its counterpart on macOS.

**In the code:** Android: `EmptyList` (`ui/library/LibraryPageContent.kt`), which the library pages share: the line in `onSurfaceVariant`, centred by `ListPlaceholder`, with its action straight under it, with no space between.
