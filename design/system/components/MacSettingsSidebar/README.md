# MacSettingsSidebar

The Settings window's list of panes, on an inset panel. The window's traffic lights sit over its top; `MacSettingsWindow` draws them and the sidebar.

| Prop | |
| --- | --- |
| `pane` | The open pane, marked current. |
| `status`, `statusPane`, `statusTone` | A chip with the words `status` on the row of `statusPane`, in `statusTone` (warning unless given): Library's Offline. |

**In the code:** macOS: the sidebar `List` in `SettingsView` (`Views/Settings/SettingsView.swift`).
