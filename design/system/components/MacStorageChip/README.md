# MacStorageChip

The Storage column, as `StorageStateLabel` draws it: one chip, or several in a row. At rest: Local, Pinned, In storage. Failed, in danger: Download failed, Move failed naming the `file` that changed since import, Couldn't make local. Waiting: Queued or Queued to make local, `ahead` (left out at 0), then the `wait`. Moving: Uploading, Pinning and Making local… fill with `progress` and take a `wait` after them; Copying here. A `wait` is a warning chip with its words: Paused, Waiting for network, Waiting for Wi-Fi, Retrying at 6:40 PM, Waiting for 01 Harbor Lights.flac. A transfer's own label shows as `transfer`.

**In the code:** macOS: `StorageStateLabel`, `QueuedPinChips`, `UploadWaitChip` and `StorageProgressChip` (`Views/Storage/Table/`).
