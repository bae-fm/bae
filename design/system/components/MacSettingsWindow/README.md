# MacSettingsWindow

The Settings window, 690 by 600 pt: its traffic lights at the top left over the panes' sidebar (`MacSettingsSidebar`), beside the open pane under its heading (`MacPageHeading`). The pane holds its `MacSection`s over `MacBox`es. The open pane's sidebar row is marked current (`aria-current`), as the app's sidebar marks that row selected.

| Prop | |
| --- | --- |
| `pane` | The open pane. |
| `title` | The pane's heading. Every pane has one. |
| `back` | On a page inside a pane, the pane the back button returns to. |
| `status`, `statusPane`, `statusTone` | A chip on a pane's sidebar row, as `MacSettingsSidebar` takes them. |
| `width`, `height` | The window's size, 690 by 600 unless given. A board gives a taller height to show more of a pane than fits. |
| `front` | The window stands in front of another window, so it casts the shadow macOS draws under the front window. |

**In the code:** macOS: `MacSettingsWindow` (`Views/Settings/MacSettingsWindow.swift`), with `MacSettingsSidebar` beside the chosen pane.
