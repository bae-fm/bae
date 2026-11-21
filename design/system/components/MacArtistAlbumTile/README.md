# MacArtistAlbumTile

An album on an artist's page: cover, title, year. Smaller than the library's tile, on a grid of 140pt and up, 12pt apart.

`unplayable`: when only another device holds the album's files, the cover and words show dimmed at `opacity-dimmed`, and the tile still opens the album.

**In the code:** macOS: `MacArtistAlbumTile` (`Views/Library/Browse/MacArtistAlbumTile.swift`) in `MacArtistAlbumGrid`. `MacArtistAlbumTile.swift` has no unplayable state yet.
