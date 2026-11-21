# MacFileRow

The inspector shows a dashed circle for a file waiting to upload (`queued`), to pin (`pin-queued`) or to move to This Mac (`move-queued`), and a progress ring while bae sends, pins or moves the file to This Mac (`moving`). The ring and filename use the accent color. The row also shows the file size. A file whose original is missing has a warning mark (`waiting`), and one whose original changed a danger mark (`error`).

The mark is the only thing on the row that says where the file stands, so VoiceOver reads it by its words: Queued to upload, Queued to pin, Queued to move to This Mac, Uploading, Pinning, Moving to This Mac…, Waiting for a file or Could not move to the cloud.

**In the code:** macOS: `MacFileRow` (`Views/Storage/MacFileRow.swift`).
