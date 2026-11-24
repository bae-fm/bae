# MacReleaseFileTable

A release's files in its Storage sheet: the table of each file's name, audio format, size and kind. The sheet draws the Files heading (`MacEyebrow`) over it, as `ReleaseStorageSheet` does. The name is in the text colour, the audio format and size in secondary text and the kind in tertiary. Name, Size and Kind sort the table; Audio doesn't. Every heading is a button, as each header cell of AppKit's table is.

| Prop | What it does |
| --- | --- |
| `files` | One `name\|audio\|size\|kind` per file, separated by semicolons. The audio is the format as the app writes it (`FLAC · 44.1 kHz · 16‑bit · Stereo`), cut to the column's width with an ellipsis. A file that isn't audio, such as the cover or a CUE sheet, leaves its audio empty. |
| `sort` | The column the table sorts by and its direction, `Name:up` unless given (`Size:down`, `Kind:up`), marked in that column's header. |

**In the code:** macOS: `MacReleaseFileTable` (`Views/Library/AlbumDetail/MacReleaseFileTable.swift`) in `ReleaseStorageSheet`, an AppKit `NSTableView`.
