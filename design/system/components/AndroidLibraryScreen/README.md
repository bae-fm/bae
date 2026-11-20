# AndroidLibraryScreen

The library's home: the view name as a menu (Albums, Artists, Composers), 32dp at the top and 22dp once scrolled (`collapsed`), then Search, Activity and ⋮; the content; the now playing bar. `searching` swaps the header for the search field.

The screen draws a dialog or sheet placed among its content over the whole screen: its scrim covers the bars and the now playing bar too, and a dialog centres on the screen. A board never wraps the screen in a box to position it.

The screen draws its header with `AndroidLibraryHeader`, which takes `view`, `collapsed`, `searching`, `search`, `sync`, `avatar` and `avatarImage`.

**In the code:** Android: `AndroidLibraryScreen` (`ui/library/AndroidLibraryScreen.kt`), taking the browser state and the activity button, content and now playing bar as slots, with `AndroidLibraryHeader`, `LibraryMoreMenu` and `AndroidSearchBar` (`ui/library/`) as its components; `AndroidLibraryScreenContainer`, in the same file, reads the session for it. (`LibraryScreen` is the navigator holding the browser or a pushed destination.)
