# MacStorageToolbar

The Storage Manager's toolbar, past the traffic lights: the window's title, All, Local and Cloud (`MacSegmentedControl`), the Filter field (`MacSearchField`) and the Inspector toggle. `MacStorageWindow` draws it.

| Prop | |
| --- | --- |
| `filter` | All, Local or Cloud; All unless given. |
| `search` | The text typed in the Filter field. |
| `inspector` | The Inspector toggle is on while the inspector shows. |
| `disabled` | No library is open: the filter, the field and the toggle are off. |

**In the code:** macOS: `MacStorageToolbar` (`Views/Storage/MacStorageToolbar.swift`).
