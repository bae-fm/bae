# MacRecordLinks

Every catalog that describes a release, as a wrapped row of links to each catalog's page, in the order core lists them: the catalog's name and an arrow out. The row wraps when the catalogs don't fit its width, 12pt between links and 4pt between lines, the same wherever it's drawn. `records` lists the catalog names, comma-separated.

Drawn in the import draft's records line (with Unlink at its end), in a search result's card, and in the open album panel's records popover.

**In the code:** macOS: `MacRecordLinks` (`Views/Components/MacRecordLinks.swift`), in `ImportReleaseHeader` (the import draft) and `ReleaseRecordsCard` (the album panel's popover).
