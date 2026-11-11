# AndroidLibraryScreen

The library's home: the view name as a menu (Albums, Artists, Composers), 32dp at the top and 22dp once scrolled (`collapsed`), then Search, Activity and ⋮; the content; the now playing bar. `searching` swaps the header for the search field.

**In the code:** Android: `AndroidLibraryScreen` (`ui/library/AndroidLibraryScreen.kt`), taking the browser state and the activity button, content and now playing bar as slots, with `LibraryHeader`, `LibraryMoreMenu` and `LibrarySearchBar` (`ui/library/`) as its parts; `AndroidLibraryScreenContainer`, in the same file, reads the session for it. (`LibraryScreen` is the navigator holding the browser or a pushed destination.)
