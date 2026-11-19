# MacPlaylistHeader

A playlist page's head, laid out as the album page's: the playlist's picture (`covers`) beside a column of the name (up to two lines), the owner (`MacPersonTag`, strong), the facts line (`count` and `length`), then Play and the ⋯ under the text, at its start.

| Prop | What it does |
|---|---|
| `options` | The owner's actions in the ⋯ menu, after Shuffle, which everyone has. `optionsOpen` draws the menu open, `highlighted` the item under the pointer. |
| `sharing` | On the person's own playlist, who sees it: a lock for Only me, two people for Everyone, at the facts line's end. A playlist someone else shared shows nothing about sharing. |

**In the code:** macOS: none yet; `PlaylistDetailPane` (`Views/Library/Playlists/PlaylistDetailPane.swift`) draws it inline.
