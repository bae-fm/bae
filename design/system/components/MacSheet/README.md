# MacSheet

A panel rounded on every corner, just under the title bar, with its title bold at the top of the content; the window behind isn't dimmed. It draws the face every Mac sheet shares (`bae-m-face-sheet`): the raised colour, the panel's corners and a shadow, with no edge. `MacModal`, `MacAlert` and `MacSidePause` draw the same face. Ends with a `MacSheetFooter`. Cancel goes back one screen, and closes the sheet on the first one. No Back buttons.

`message` is the line under the title, in secondary text, as an alert's message: what the sheet asks for or reports. It's never drawn by hand in the body.

**In the code:** macOS: `MacSheet` (`Views/Components/MacSheet.swift`), the sheet's face, inside SwiftUI's `.sheet`; some sheets still draw their own header (`JoinRequestSheet`, `SyncSetupWizard`).
