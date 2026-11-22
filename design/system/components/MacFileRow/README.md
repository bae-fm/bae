# MacFileRow

A file in the inspector: dashed circle while queued to upload (`queued`) or to pin (`pin-queued`), a ring filling while sending or pinning (`MacProgressRing`, in the activity or pinning colour), the name in that color, the size. A file whose original is missing has a warning mark (`waiting`), and one whose original changed a danger mark (`error`).

The mark is the only thing on the row that says where the file stands, so VoiceOver reads it by its words: Queued to upload, Queued to pin, Uploading, Pinning, Waiting for a file or Could not move to the cloud.

**In the code:** macOS: `MacFileRow` (`Views/Storage/MacFileRow.swift`).
