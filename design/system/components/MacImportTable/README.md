# MacImportTable

The mapping table: Source | Track | Title | Artist | Length, then side headers (`MacEyebrow`), CUE captions and track rows. The table spaces each side header: 2pt above the first and 16pt above the others, 6pt under each.

The columns keep a floor: Source 160, Track 52, Title 120, Artist 100 and Length 88 points, with the gaps 560 in all. A pane narrower than that scrolls the table sideways, as the app does.

**In the code:** macOS: `MacImportTable` (`Views/Import/Mapping/MacImportTable.swift`), in `MacReleaseEditor`, in the import pane and the Edit release sheet alike.
