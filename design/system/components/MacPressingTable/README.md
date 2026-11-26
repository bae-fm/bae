# MacPressingTable

Find online's pressings as one table (decision #2562): each album, a `MacReleaseGroup`, with its pressings below it, on one grid that every row shares, so each column starts at one x on every row.

The columns are the year, the label, the catalog number, the country, the format and the details, then one for each identifier kind the band found (`kinds`, comma-separated in the band's order), then the chevron. The identifier columns carry their names once, above the first album, and the other columns have no headings. A run that found its releases by the title search, and a typed search, have no identifier columns.

The label column is as wide as its widest label, at least 200pt, with 8pt more space after it, and the details column takes the spare width. Columns are 16pt apart, and the chevron is 40pt from the last column.

**In the code:** macOS: `ReleaseGroupListView` (`Views/Import/Search/ReleaseGroupListView.swift`).
