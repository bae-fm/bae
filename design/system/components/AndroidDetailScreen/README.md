# AndroidDetailScreen

An album's, artist's or composer's page: the back arrow alone, `menu` for the page's options, the content in a 16dp column (`flush` when its rows carry their own padding, as an artist's page does), the now playing bar.

**In the code:** Android: `AndroidDetailScreen` (`ui/library/AndroidDetailScreen.kt`), taking the page's content, its menu as `actions` and the now playing bar as a slot; the album, artist, composer and work pages place it.
