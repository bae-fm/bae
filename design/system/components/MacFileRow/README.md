# MacFileRow

A file in the inspector: dashed circle while queued to upload (`queued`), to pin (`pin-queued`) or to move to This Mac (`move-queued`), a ring filling while sending, pinning or moving to This Mac (`moving`) (`MacProgressRing`, in the activity colour for an upload and the pinning colour for a download), the name in that color, the size. A file whose original is missing has a warning mark (`waiting`), and one whose original changed a danger mark (`error`).

The mark is the only thing on the row that says where the file stands, so VoiceOver reads it by its words: Queued to upload, Queued to pin, Queued to move to This Mac, Uploading, Pinning, Moving to This Mac…, Waiting for a file or Could not move to the cloud.

**In the code:** macOS: `MacFileRow` (`Views/Storage/MacFileRow.swift`).
