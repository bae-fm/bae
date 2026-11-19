# MacReleasePicker

The choice of an album's release, in the album's header: one segment for each release in `releases` (comma-separated), with `selected` the one showing (the first unless given). An album of one release has no choice, so the picker draws nothing.

**In the code:** macOS: none yet; `AlbumExpansionContent` (`Views/Library/AlbumDetail/AlbumExpansionContent.swift`) draws it inline.
