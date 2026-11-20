# MacImportSidebar

Import's left column: Found / Imported / Skipped, then the filter row, then the rows or `empty`.

The column is 420 points wide, never under 410, which the four tabs and their counts need on one line in English, and never over 460. `narrow` draws it at 410, as the main window's smallest width, 900 points, leaves it.

- Each tab is one tab, named with its count: "Found, 14".
- The filter row holds the Filter field (`filter` is the text typed in it), the narrowed count, and its buttons: the identifying ring, importing and scanning, each a button that opens what it stands for, and the ⋯ button, which opens the menu of sorting, filtering and watched folders and shows a dot while a filter is on (`filtered`).

The tabs are a `MacImportTabs` and the filter row a `MacImportFilterRow`. An empty list draws `empty` as a `MacContentUnavailable`.

**In the code:** macOS: `MacImportSidebar` (`Views/Import/Candidates/MacImportSidebar.swift`), with `MacImportTabs` and `MacImportFilterRow`; `MacImportSidebarContainer` reads the stores for it.
