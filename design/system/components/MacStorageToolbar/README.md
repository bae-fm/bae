# MacStorageToolbar

The Storage Manager's toolbar, past the traffic lights: the window's title, All, Local and Cloud (`MacSegmentedControl`), the Filter field (`MacSearchField`) and the Inspector toggle. The Inspector toggle is a native toolbar item: macOS gives it a box 36pt wide and as tall as the bar, 4pt from the window's edge, with the 28pt `MacIconButton` at its centre, and VoiceOver reads the box as the Inspector button. `MacStorageWindow` draws it.

| Prop | |
| --- | --- |
| `filter` | All, Local or Cloud; All unless given. |
| `search` | The text typed in the Filter field. |
| `inspector` | The Inspector toggle is on while the inspector shows. |
| `disabled` | No library is open: the filter, the field and the toggle are off. |

**In the code:** macOS: `MacStorageToolbar` (`Views/Storage/MacStorageToolbar.swift`).
