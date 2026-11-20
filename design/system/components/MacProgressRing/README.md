# MacProgressRing

A file's transfer ring in the Storage Manager's inspector: a ring that fills clockwise from the top as the file's upload or pin goes on.

- `value`: how far the transfer has got, from 0 to 1.
- `tone`: `activity` (the default) for a file moving to or from storage, `pinning` for a file bae is pinning, each over a hairline.
- `label` names the ring for VoiceOver. Without one, the row around the ring names it.

**In the code:** macOS: `MacProgressRing` (`Views/Storage/MacProgressRing.swift`), which hosts `TransferRingNSView` (`Views/Storage/TransferRingNSView.swift`); `StorageFileMark` (`Views/Storage/MacFileRow.swift`) draws it for a moving file.
