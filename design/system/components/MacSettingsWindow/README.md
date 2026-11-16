# MacSettingsWindow

Holds the pane's `MacSection`s over `MacBox`es. `back` for a page inside a pane. The open pane's sidebar row is marked current (`aria-current`), as the app's sidebar marks that row selected.

The window is always 600 pt tall and its pane scrolls, as the app's does: a pane with more content than fits runs under the window's bottom edge, cut off, starting from the top. When a board shows content lower in the pane, set `scroll` to the pixels to scroll the pane; the heading stays in place.

**In the code:** macOS: `SettingsView` (`Views/Settings/SettingsView.swift`): the panes in a sidebar `List` on a rounded panel beside the chosen pane, the window's title bar transparent so its buttons sit in the sidebar; each pane is a `SettingsPane`, which draws its title over its form.
