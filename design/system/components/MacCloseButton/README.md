# MacCloseButton

The close control of a docked pane (the queue) and of an open album: an × in a round, quiet fill, 28pt. `MacQueuePanel` puts it at the end of its heading; `MacAlbumPanel` in its top corner.

**In the code:** macOS: `PanelCloseButton` (`Views/Components/PanelCloseButton.swift`), placed by `AlbumExpansionContent` and `QueueView`; it draws a rounded square where this draws a circle.
