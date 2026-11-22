# MacAlbumTile

Cover, title, artists, year (the year line holds its height when empty). With no `artist`, the tile draws no artists line, and the year sits under the title: under an artist's group, a tile whose only credit is that artist has no artists line. `hover` shows the options button on the cover; `open` rings the cover while its detail is open below the row; `selected` for a multiple selection.

The options button is a `MacMoreMenu` with `overlay`, labelled Album options.

`unplayable`: when only another device holds the album's files, the cover and words show dimmed at `opacity-dimmed`, and the tile still opens the album.

**In the code:** macOS: `MacAlbumTile` (`Views/Library/Grid/MacAlbumTile.swift`), with an overlay `MacMoreMenu`, the round ⋯ at the cover's top right while the pointer is over the tile.
