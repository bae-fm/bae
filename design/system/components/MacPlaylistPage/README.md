# MacPlaylistPage

A playlist's page in the main window. Clicking a playlist in the side pane's Playlists tab opens it in place of the library's view, and the side pane stays open beside it with that playlist selected. The page shows the playlist's picture, its name and a line of owner, track count, length and who sees it. Play, Shuffle and the owner's ⋯ (`options`, named Playlist options: Edit playlist…, Delete playlist…) follow. × at the end (Close playlist) closes the page, and the window shows the library's view again. The tracks (`MacListRow`, numbered from 1) are its children.

**In the code:** macOS: `PlaylistDetailPane` (`Views/Library/Playlists/PlaylistDetailPane.swift`) draws the page as the detail of the Playlists view's split, with no ×; the window has no page in place of the view yet.
