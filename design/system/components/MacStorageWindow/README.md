# MacStorageWindow

The Storage Manager window, with its own frame and traffic lights: the toolbar (`MacStorageToolbar`: title, All / Local / Cloud, the Filter field, the Inspector toggle), then the release list and, with `inspector`, the inspector. The list is the `MacTable` child with the `MacStorageFooter` child under it; the other children are the inspector's. With no library open, the first child is the list alone. With `disabled` (no library open), the toolbar passes `off` to the segmented control, the Filter field and the Inspector button: each control draws its own off state. `empty`: the filter matches nothing, and the window says No matches over the table as a `MacContentUnavailable`.

**In the code:** macOS: `MacStorageWindow` (`Views/Storage/MacStorageWindow.swift`), which lays `MacContentUnavailable` over `MacTable` while a search finds no rows.
