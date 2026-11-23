# MacReleaseFileTable

A release's files in its Storage sheet: the Files heading (`MacEyebrow`) over the table of each file's name, audio format, size and kind. The name is in the text colour, the audio format and size in secondary text and the kind in tertiary. Name, Size and Kind sort the table; Audio doesn't.

| Prop | What it does |
| --- | --- |
| `files` | One `name\|audio\|size\|kind` per file, separated by semicolons. A file that isn't audio, such as the cover or a CUE sheet, leaves its audio empty. |
| `sort` | The column the table sorts by and its direction, `Name:up` unless given (`Size:down`, `Kind:up`), marked in that column's header. |

**In the code:** macOS: `MacReleaseFileTable` (`Views/Library/AlbumDetail/MacReleaseFileTable.swift`) in `ReleaseStorageSheet`, an AppKit `NSTableView`.
