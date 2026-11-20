# MacTable

The Storage Manager's release table: the header, then one row a release, every other row on the hover fill and the selected row in the accent's tint.

| Prop | What it does |
| --- | --- |
| `columns` | The columns, `Title:width[:right][:up\|:down]` each, separated by commas. The first takes the room the others leave; `up` or `down` marks the column the table sorts by. |
| `rows` | The releases, one entry a row separated by semicolons, with one field a column in the columns' order, separated by `\|`: the title, the artist, the media, the storage (a `MacStorageChip` state, then its progress for a state that moves, such as `uploading 0.62`), the files, the size and the date added. The Release column draws `MacReleaseCell`, and every other column `MacStorageCell`. |
| `selected` | The numbers of the selected rows, counting from 1 and separated by commas: the one the inspector shows, or several the person picked together. |
| `loading` | How many rows are still on their way after the given ones: each draws a placeholder bar in its Release column. |

`MacStorageWindow` says No matches over the table when the filter matches nothing.

**In the code:** macOS: `MacTable` (`Views/Storage/MacTable.swift`), AppKit's `NSTableView`, with one hosted cell a column.
