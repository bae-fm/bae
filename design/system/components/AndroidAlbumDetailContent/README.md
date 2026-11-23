# AndroidAlbumDetailContent

The album page below its bar, inside `AndroidDetailScreen`: the page's blocks (the header, the release picker, the catalogs' links, the pin control, the tracks by side or disc, the credits, the files and the audio line) in a column 16dp in from the screen's edges, each block 8dp from the next. The track rows touch one another. The page's dialogs and sheets go beside this column, as the detail screen's own children, so that they cover the whole screen.

**In the code:** Android: `AlbumDetailContent` (`ui/albumdetail/AlbumDetailScreen.kt`), the page's `LazyColumn`, padded by `ThemeSpace.edge`, whose `block`s each end `ThemeSpace.related` above the next.
