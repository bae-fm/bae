# MacListRow

Exploration: a history or playlist row: when or position, cover, title, artist · album, who, length. While the owner drags a playlist's row, `lifted` fades the row left in its place and `dragging` raises the copy under the pointer.

`hanging`: the leading column hangs in the margin left of the row, right-aligned and `leadWidth` wide (20pt for a playlist's numbers, 56pt for History's times), so the covers start at the list's edge, and the credit line sits 3pt under the title, as a playlist's page in the side pane draws its tracks.

The row draws who with `MacPersonTag`.

`hover`: the row under the pointer tints; with `remove`, the label of a button that takes the row out of its list (the queue's Remove from queue), that button joins the row's end.

**In the code:** macOS: `MacListRow` (`Views/Components/MacListRow.swift`), which the playlist page draws for a track and History draws for a play, with who.
