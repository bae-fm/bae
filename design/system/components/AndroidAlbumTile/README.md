# AndroidAlbumTile

Cover, title, one line under it: the artists, or when grouped by artist the year and "with <credit>" when it differs. The whole tile is one button that opens the album, and TalkBack reads it as the title and the line.

**In the code:** Android: `AndroidAlbumTile` (`ui/library/AndroidAlbumTile.kt`), placed by the library grid (`ui/library/LibraryGrid.kt`), which words its line from the album's row (`groupedCardLine` in a group).
