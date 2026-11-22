# MacTrackRow

Position, title, duration; the artist under the title on a compilation. The current track's title is accent with a speaker in place of its number; `hover` shows play (or pause) there.

The row draws its lead with `MacTrackLead`.

| Prop | What it does |
|---|---|
| `picked` | The track is among the tracks picked together: the row sits on the soft accent. |
| `rating` | The stars you gave the track (1 to 5). The row always keeps a slot for them after it, as every album list does, and the slot is empty when you gave none, so every row's duration lines up. |
| `ratingSystem` | The library's system for tracks: `heart` puts one `Heart` in the slot when `rating` is three or more, and `none` draws no slot. |

**In the code:** macOS: `MacTrackRow` (`Views/Library/AlbumDetail/TrackRowView.swift`), the row as drawn from its values, with `picked`; `TrackRowView` there follows the pointer and the jump's flash for it and draws the stars' slot beside it.
