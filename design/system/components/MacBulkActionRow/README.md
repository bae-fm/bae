# MacBulkActionRow

One action in the bulk pane, for several selected folders: its glyph (`icon`, the symbol the app draws for the action) at the medium icon size, its name (`label`), then a chip with how many of the selected folders it applies to (`count`). With none, the whole row dims. `strong` sets the action in the accent, as Import and Combine as one release. `to` puts Import's destination pop-up before the count, on Import's own line. The row's button then holds only the glyph and the name, so the pop-up is a control of its own.

**In the code:** macOS: `MacBulkActionRow` (`bae-macos/bae/bae/Views/Import/MacBulkActionRow.swift`), in `MacImportBulk`.
