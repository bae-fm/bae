# MacBulkActionRow

One action in the bulk pane, for several selected folders: its glyph (`icon`), its name (`label`), then a chip with how many of the selected folders it applies to (`count`). With none, the whole row dims. `strong` sets the action in the accent, as Import and Combine as one release. A child sits before the count: Import's destination, on Import's own line.

**In the code:** macOS: drawn in `ImportCandidateBulkSelectionPane` (`bae-macos/bae/bae/Views/Import/ImportCandidateBulkSelectionPane.swift`)
