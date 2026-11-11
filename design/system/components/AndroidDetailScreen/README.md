# AndroidDetailScreen

An album's, artist's or composer's page: the back arrow alone, `menu` for the page's options, the content in a 16dp column (`flush` when its rows carry their own padding, as an artist's page does), the now playing bar.

**In the code:** Android: each detail screen lays out `LibraryDetailTopBar` (`ui/library/LibraryChrome.kt`), its content and `NowPlayingBar` itself (`AlbumDetailScreen.kt`, `ArtistDetailScreens.kt`, `ComposerDetailScreens.kt` twice); no one view.
