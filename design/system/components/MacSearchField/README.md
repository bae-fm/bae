# MacSearchField

A search or filter field: the magnifier, then the text.

- `kind`: `search` (the default) is the title bar's rounded field, 300pt wide. `filter` is a toolbar's 160pt field, as the Storage Manager's. `inline` is a field with no box that fills the rest of its row, as the import list's filter row.
- `label` names the field and shows while it is empty: Search unless given, Filter in a filter row.
- `value` is the text typed.
- `focused` rings the search field in the accent while it has focus.
- `off` dims a field that has nothing to filter, and the field takes no typing.

**In the code:** macOS: `MacSearchField` (`bae-macos/bae/bae/Views/Chrome/MacSearchField.swift`), in `MacTitleBar` for `search`, and in `MacStorageToolbar` and the import list's `ImportListFilterField` for `filter`.
