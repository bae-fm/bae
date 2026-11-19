# MacTrackRow

Position, title, duration; the artist under the title on a compilation. The current track's title is accent with a speaker in place of its number; `hover` shows play (or pause) there.

The row draws its lead with `MacTrackLead`.

| Prop | What it does |
|---|---|
| `picked` | The track is among the tracks picked together: the row sits on the soft accent. |
| `rating` | The row is in an album's list, which keeps a slot for the track's stars after the row: the stars you gave it (1 to 5), or 0 for an empty slot, so every row's duration lines up. |

**In the code:** macOS: `MacTrackRow` (`Views/Library/AlbumDetail/TrackRowView.swift`), the row as drawn from its values, with `picked`; `TrackRowView` there follows the pointer and the jump's flash for it and draws the stars' slot beside it.
