# MacSavePanelFormat

The Save as panel's Format line: "Format:" and the format pop-up, centred under the panel's fields. The line is the panel's accessory view, 250 by 34 pt, with the pop-up 190 by 24 pt, as the app lays it out; the panel draws any line above it. The pop-up is AppKit's `NSPopUpButton`, which draws itself and the menu it opens: a filled bezel with the format 12 pt in and the up-and-down chevrons at its end.

| Prop | |
| --- | --- |
| `value` | The chosen format. |
| `open`, `items`, `chosen` | The menu the pop-up opens, with its checked format on the pop-up's line: its comma-separated formats (`-` for a separator), and the format it checks when its words differ from `value`. Two boards show the open menu; a shot of the app can't open a native menu, so the open rows get no shot. |

**In the code:** macOS: `MacSavePanelFormat` (`bae-macos/bae/bae/Views/Library/AlbumDetail/MacSavePanelFormat.swift`), an `NSView` that `TrackSavePanel` sets as the save panel's accessory view.
