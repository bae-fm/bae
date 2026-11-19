# MacPlaceholderLine

A grey bar where a line of text or a picture will be while it loads: the loading rows of a library list, the queue and the Storage Manager's table.

| Prop | |
| --- | --- |
| `width`, `height` | The bar's size in points. |
| `radius` | A picture's corners (a cover's 6pt); a bar has the bar radius otherwise. |
| `fixed` | Keeps its width in a row that shrinks its other parts. |

**In the code:** macOS: `StorageRowPlaceholderCell` (`Views/Storage/Table/`) and the bars in `QueuePlaceholderRow` (`Views/Playback/Queue/QueuePlaceholderRow.swift`).
