# MacSavePanelFormat

The Save as panel's Format line: "Format:" and the format pop-up, centred under the panel's fields, with a hairline above it. The pop-up is AppKit's `NSPopUpButton`, which draws itself and the menu it opens.

| Prop | |
| --- | --- |
| `value` | The chosen format. |
| `open`, `items`, `chosen` | The menu the pop-up opens, as `MacPopUpButton` draws it: its comma-separated formats (`-` for a separator), and the format it checks. Two boards show the open menu; a shot of the app can't open a native menu, so the open rows get no shot. |

**In the code:** macOS: `TrackSavePanel` (`bae-macos/bae/bae/Views/Library/AlbumDetail/TrackSavePanel.swift`) draws the line, and AppKit draws its `NSPopUpButton`.
