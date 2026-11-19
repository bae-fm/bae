# MacSearchField

A search or filter field: the magnifier, then the text.

- `kind`: `search` (the default) is the title bar's rounded field, 300pt wide. `filter` is a toolbar's 160pt field, as the Storage Manager's. `inline` is a field with no box that fills the rest of its row, as the import list's filter row.
- `label` names the field and shows while it is empty: Search unless given, Filter in a filter row.
- `value` is the text typed.
- `focused` rings the search field in the accent while it has focus.
- `off` dims a field that has nothing to filter, and the field takes no typing.

**In the code:** macOS: `LibrarySearchField` (`bae-macos/bae/bae/Views/Chrome/LibrarySearchField.swift`) for `search`. `filter` is AppKit's search field, which SwiftUI's `searchable(placement: .toolbar)` puts in the Storage Manager's toolbar (`View+StorageManagerToolbar.swift`); its rows draw only what that field shows: empty with Filter, the typed text, and off, which the toolbar draws as `OffFilterField`, the magnifier and the word Filter dimmed on the field's fill.
