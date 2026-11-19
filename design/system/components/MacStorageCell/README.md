# MacStorageCell

A cell of the Storage Manager's table in every column but Release: the release's artist, media or date added as text, its files or size at the column's end in tabular figures, or its storage as a `MacStorageChip`. `MacTable` draws it.

| Prop | What it does |
| --- | --- |
| `value` | The cell's text. |
| `numeric` | The value is a number (files, size): it sits at the column's end in tabular figures. |
| `state` | The storage column's state, which `MacStorageChip` draws in place of `value`. |
| `progress` | With a state that moves (uploading, pinning, detaching), how far it has got, from 0 to 1. |

**In the code:** macOS: `StorageReleaseCell` (`Views/Storage/Table/StorageReleaseCell.swift`) for every column but `.release`, which it hands to `MacReleaseCell`.
