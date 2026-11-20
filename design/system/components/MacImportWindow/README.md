# MacImportWindow

The Import window's content: the import list beside the pane.

- The first child is the list column, a `MacImportSidebar`.
- The other children fill the pane, which pads them and stacks them with the group space between: the folder line, the draft card, the track table, the files.
- `noSelection` draws the pane with no folder picked: Select a folder, and a line that asks the person to choose a scanned folder.

**In the code:** macOS: `MacImportWindow` (`bae-macos/bae/bae/Views/Import/MacImportWindow.swift`).
