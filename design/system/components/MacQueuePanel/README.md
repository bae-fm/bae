# MacQueuePanel

The side pane docked at the window's right (420pt). Its top holds the Queue, Playlists and History tabs (`tab`, Queue when unset) and Close; on Playlists a + button (New playlist…) starts the top. On Queue come the Now playing card while something plays, then the sections, or with `empty` "Queue is empty" / "Drag tracks here or play an album". On Playlists and History the children are the tab's content: the playlists or an open playlist's page (`MacPlaylistPage`), or the plays under their day headings. Put it as a child of `MacMainWindow`.

**In the code:** macOS: `MacQueuePanel` (`Views/Playback/Queue/MacQueuePanel.swift`) draws the panel from the lanes and now-playing it is given; `MacQueuePanelContainer` reads the stores for it.
