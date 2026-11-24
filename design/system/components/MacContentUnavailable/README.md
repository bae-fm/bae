# MacContentUnavailable

SwiftUI's empty-pane message: a title, an optional line under it, and the children as its actions in a row under the text, centered in what's left of the pane. With no actions, the text sits above the pane's middle, as in the app: 3pt for a title alone, 9pt with a line under it. No glyph sits above the title, not even for a pane that failed to load: the title says what happened.

**In the code:** macOS: `MacContentUnavailable` (`Views/Components/MacContentUnavailable.swift`).
