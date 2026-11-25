# MacStorageToolbar

The Storage Manager's toolbar, as macOS 26 draws a window's toolbar: 52pt tall with no line under it and the traffic lights centred in it. Past the lights come the window's title in the heading type, All, Local and Cloud (`MacSegmentedControl`) at the window's centre, then the Filter field (`MacSearchField`) and the Inspector toggle side by side at the end. The Inspector toggle is a native toolbar item: macOS gives it a box 36pt wide and as tall as the bar, 4pt from the window's edge, with the 28pt `MacIconButton` at its centre, and VoiceOver reads the box as the Inspector button. `MacStorageWindow` draws it, and the window draws the traffic lights. The preview draws each row in an empty Storage Manager window 172pt tall, as the shots take the toolbar.

| Prop | |
| --- | --- |
| `filter` | All, Local or Cloud; All unless given. |
| `search` | The text typed in the Filter field. |
| `inspector` | The Inspector toggle is on while the inspector shows. |
| `disabled` | No library is open: the filter, the field and the toggle are off. |

**In the code:** macOS: `MacStorageToolbar` (`Views/Storage/MacStorageToolbar.swift`).
