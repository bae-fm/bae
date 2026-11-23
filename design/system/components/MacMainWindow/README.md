# MacMainWindow

The main window every library and playback board sits in: the title bar (Library and Import, search, Activity, Settings), the content, and the now playing bar along the bottom. The `track…` props feed the bar; leave `trackTitle` out when nothing has played. A sheet or an alert over the window goes among its children, so its scrim dims the whole window, title bar and now playing bar included.

The window draws its own frame, the rounded window with its traffic lights 15 pt from its left and 16 pt from its top, where the app's `TrafficLightOffset` moves them, as the app's `MainWindowChrome` does; its title bar with `MacTitleBar` (`section`, `search`, `searchFocused`, `sync`, `avatar`, `avatarImage` go to it) and its bar with `MacNowPlayingBar`.

**In the code:** macOS: `MacMainWindow` (`Views/MacMainWindow.swift`), the layout, with `MacMainWindowContainer` (`Views/MacMainWindowContainer.swift`) reading the stores for it and `MacTitleBar`, `MacSearchField` and `MacActivityButton` (`Views/Chrome/`).
