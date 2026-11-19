# MacPlaylistPage

A playlist's page inside the side pane. Clicking a playlist in the pane's Playlists tab opens its page there, and the window keeps the library's view. The page starts with the round back button (Back to Playlists), which returns to the list, and the playlist's name. Under them come its picture beside a line of owner, track count and length, over who sees it; then Play, Shuffle and the owner's ⋯ (`options`, named Playlist options: Edit playlist…, Delete playlist…). The tracks follow as its children, compact `MacListRow`s numbered from 1. The pane's tabs stay switchable while a playlist is open, and the Playlists tab shows the open playlist again when the person comes back to it. Put it as the child of a `MacQueuePanel` on the Playlists tab.

**In the code:** macOS: `PlaylistDetailPane` (`Views/Library/Playlists/PlaylistDetailPane.swift`) draws the page as the detail of the Playlists view's split, in the window; the side pane holds no page yet.
