# MacItem

The row piece every macOS row with a leading icon is built from: `lead` (an icon, a status glyph, a progress mark), the content, then `trail` (a size, a time, a cancel mark). The leading column is `size-lead` wide and as tall as the first line, the icon centred in it; the content starts `space-related` after it. The column stays when `lead` is empty, so a row with no icon keeps the same text edge as its neighbours. It has no inset of its own: the row it sits in places it.

`MacFileRow`, `MacDownloadRow`, the Activity panel's rows and `MacNotice`'s glyph are all built on it, so wherever they sit together, their words share one edge.

**In the code:** macOS: `RowItem` (BaeKit `Views/RowItem.swift`).
