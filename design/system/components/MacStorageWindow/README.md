# MacStorageWindow

The Storage Manager window: title, All / Local / Cloud, the Filter field, the Inspector toggle; the release list (first child) and, with `inspector`, the inspector (the other children). With `disabled` (no library open), the window passes `off` to the segmented control and the Inspector button and dims its own Filter field: each control draws its own off state.

**In the code:** macOS: `StorageManagerView` (`Views/Storage/StorageManagerView.swift`).
