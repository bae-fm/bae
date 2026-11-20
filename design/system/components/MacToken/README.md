# MacToken

One value of a file name pattern, drawn as an accent chip: the value's name in the accent, on the accent's tint, with small corners and no ×. `MacFileNameField` draws each value of its pattern as one; the dashed values after "Add:" under the field are its own buttons, not tokens. The child is the value's name.

**In the code:** macOS: no view draws the accent chip; `SaveNameTokenField` (`Views/Settings/Formats/SaveNameTokenField.swift`) has AppKit's `NSTokenField` draw the tokens in the system's blue.
