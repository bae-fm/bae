# MacImportSidebar

Import's left column: Found / Imported / Skipped, then the filter row, then the rows or `empty`.

- Each tab is one tab, named with its count: "Found, 14".
- The filter row holds the Filter field (`filter` is the text typed in it), the narrowed count, and its buttons: the identifying ring, importing and scanning, each a button that opens what it stands for, and the ⋯ button, which opens the menu of sorting, filtering and watched folders and shows a dot while a filter is on (`filtered`).

The tabs are a `MacImportTabs` and the filter row a `MacImportFilterRow`. An empty list draws `empty` as a `MacContentUnavailable`.

**In the code:** macOS: `ImportCandidateListContent`, `TriageTabBar` and the filter row (`Views/Import/Candidates/`).
