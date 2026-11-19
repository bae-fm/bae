# MacToken

One value of a file name pattern, as AppKit's token field (`NSTokenField`) draws its tokens: the system's blue, the value's name in the field's font, small corners and no ×. `MacFileNameField` draws each value of its pattern as one; the dashed values after "Add:" under the field are its own buttons, not tokens. The child is the value's name. AppKit draws the token, so the preview shows only how a token looks with a short and a long name.

**In the code:** macOS: AppKit draws the tokens of the `NSTokenField` in `SaveNameTokenField` (`Views/Settings/Formats/SaveNameTokenField.swift`).
