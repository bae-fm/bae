# AndroidAlbumTile

Cover, title, one line under it: the artists, or when grouped by artist the year and "with <credit>" when it differs. The whole tile is one button that opens the album, and TalkBack reads it as the title and the line.

`unplayable` dims the cover and words at `opacity-dimmed` when this device cannot play the album's primary release. The tile still opens the album.

**In the code:** The library grid (`ui/library/LibraryGrid.kt`) passes the album's playability and credit line to `AndroidAlbumTile` (`ui/library/AndroidAlbumTile.kt`). Grouped tiles use `groupedCardLine`.
