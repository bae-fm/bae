# AndroidDetailScreen

An album's, artist's or composer's page: the back arrow alone, `menu` for the page's options, the page's content, which carries its own padding (the album page's is `AndroidAlbumDetailContent`; an artist's rows pad themselves), the now playing bar.

The screen draws a dialog or sheet placed among its content over the whole screen: its scrim covers the bars and the now playing bar too, and a dialog centres on the screen. A board never wraps the screen in a box to position it.

The screen draws its bar with `AndroidTopBar`, with no title: the back button, named for where it goes (`back`: the page's name, or the library), and the ⋮ when `menu` is set.

**In the code:** Android: `AndroidDetailScreen` (`ui/library/AndroidDetailScreen.kt`), taking the page's content, its menu as `actions` and the now playing bar as a slot; the album, artist, composer and work pages place it.
