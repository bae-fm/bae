# MacMainWindow

The main window every library and playback board sits in: the title bar (Library and Import, search, Activity, Settings), the content, and the now playing bar along the bottom. The `track…` props feed the bar; leave `trackTitle` out when nothing has played.

**In the code:** macOS: `MacMainWindow` (`Views/MacMainWindow.swift`), the layout, with `MacMainWindowContainer` (`Views/MacMainWindowContainer.swift`) reading the stores for it and `TitleBar`, `LibrarySearchField` and `ActivityButton` (`Views/Chrome/`).
