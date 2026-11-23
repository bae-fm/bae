# MacBulkActionRow

One action in the bulk pane, for several selected folders: its name (`label`), then a chip with how many of the selected folders it applies to (`count`). The row draws no glyph, since a glyph would only repeat the name. With none, the whole row dims. `strong` sets the name in the strong type and the count in the accent, as Import and Combine as one release. `to` puts Import's destination pop-up before the count, on Import's own line. The row's button then holds only the name, so the pop-up is a control of its own.

**In the code:** macOS: `MacBulkActionRow` (`bae-macos/bae/bae/Views/Import/MacBulkActionRow.swift`), in `MacImportBulk`.
