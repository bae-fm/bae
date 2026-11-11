# AndroidAlbumGrid

Two columns on a phone (the app fits as many 150dp columns as the width allows), 12dp apart and around.

**In the code:** Android: `AndroidAlbumGrid` (`ui/library/LibraryGrid.kt`), taking its rows by index, `GridCells.Adaptive(minSize = 150.dp)`; `AndroidAlbumGridContainer`, in the same file, reads the album page for it.
