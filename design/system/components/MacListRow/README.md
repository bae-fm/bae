# MacListRow

Exploration: a history or playlist row: when or position, cover, title, artist · album, who, stars, length. While the owner drags a playlist's row, `lifted` fades the row left in its place and `dragging` raises the copy under the pointer.

`hanging`: the leading column hangs in the margin left of the row, right-aligned and `leadWidth` wide (20pt for a playlist's numbers, 56pt for History's times), so the covers start at the list's edge, and the credit line sits 3pt under the title, as a playlist's page in the side pane draws its tracks.

The row draws who with `MacPersonTag`.

**In the code:** macOS: `MacListRow` (`Views/Components/MacListRow.swift`), which the playlist page draws for a track; no view draws the history row, with who and stars, yet.
