# AndroidContentUnavailable

An empty or unavailable screen's message: one line of body text in the secondary colour, centred in what's left of the screen, with no glyph above it ("No albums yet", "No plays yet"). The children are its actions, in a row 12dp under it, as `MacContentUnavailable` spaces its own. `MacContentUnavailable` is its counterpart on macOS.

**In the code:** Android: `AndroidContentUnavailable` (`ui/components/AndroidContentUnavailable.kt`), the line with its actions under it. The library pages centre it with `ListPlaceholder` (`ui/library/LibraryPageContent.kt`).
