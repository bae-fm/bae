# MacRecordLinks

Every catalog that describes a release, as a wrapped row of links to each catalog's page, in the order core lists them: the catalog's name and an arrow out, a link. The row wraps when the catalogs don't fit its width, 12pt between links and 4pt between lines, the same wherever it's drawn. `records` lists the catalog names, comma-separated.

The import draft draws Unlink beside the row; a library release's editor card, a search result's card and the open album panel's records popover draw the row alone.

**In the code:** macOS: `MacRecordLinks` (`Views/Components/MacRecordLinks.swift`), in `MacReleaseEditor` and `ReleaseRecordsCard` (the album panel's popover).
