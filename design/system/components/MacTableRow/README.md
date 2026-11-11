# MacTableRow

One row of a `MacTable`: its cells as children. `selected`; `placeholder` for a row not loaded yet.

**In the code:** macOS: a row of `StorageTableView`'s `NSTableView`, each cell a `StorageReleaseCell` for its column, `StorageRowPlaceholderCell` for a row not loaded (`Views/Storage/Table/`).
