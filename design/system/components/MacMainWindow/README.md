# MacMainWindow

The main window every library and playback board sits in: the title bar (Library and Import, search, sync activity, Settings), the content, and the now playing bar along the bottom. The `track…` props feed the bar; leave `trackTitle` out when nothing has played.

**In the code:** macOS: `MainAppView` (`Views/MainAppView.swift`) with `TitleBar`, `LibrarySearchField` and `SyncActivityButton` (`Views/Chrome/`).
