# AndroidLibraryHeader

The library's header on Android: the view's title (`view`), which opens the view menu, then search, the Activity button (`AndroidActivityButton`), ⋮ and the person's picture or initials, which opens their menu: Add device, Invite someone, Playlists, History, Settings.

| Prop | What it does |
|---|---|
| `collapsed` | The grid has scrolled: a smaller title on the surface colour. |
| `searching`, `search` | The header becomes the search bar (`AndroidSearchBar`), with what the person typed. |
| `sync` | The Activity button's state: moving, attention or paused. |
| `avatar`, `avatarImage` | The person's name and picture. |

**In the code:** Android: `LibraryHeader` (`ui/library/LibraryHeader.kt`).
