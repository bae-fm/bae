# MacStorageChip

The Storage column, as `StorageStateLabel` draws it: one chip, in pinning's words (README, Words). At rest: Local, Pinned, In cloud. Moving: Uploading, Pinning and Making local… fill with `progress`. Waiting: Queued to upload, Queued to pin, Queued to make local; never a bare Queued. An upload held by a missing original reads Waiting for that `file`, in warning: the release's own problem, which only restoring the file fixes. Failed, in danger: Couldn't pin, Move failed naming the `file` that changed since import, Couldn't make local. Nothing says how many wait ahead or what holds the queue back (Paused, Waiting for network): what holds a whole queue back shows once, on `MacStorageFooter`, not on each release.

**In the code:** macOS: `StorageStateLabel`, with `QueuedUploadChip`, `StorageUploadingChip`, `StoragePinningChip`, `StorageMakingLocalChip` (`Views/Storage/Table/StorageStateLabel.swift`) and `StorageProgressChip` (`StorageProgressChip.swift`).
