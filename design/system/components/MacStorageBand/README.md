# MacStorageBand

A release's storage in its Storage sheet, in the order `StorageStatusBand` draws it: the state line (Local, Pinned, In storage, an upload with its bar, or the phase of files only waiting to upload with its `wait` chip); the download line (a pin queued, with `ahead` and `wait`, pinning with its `wait` under the progress line, Download failed with its `error`, or Copying here); why this Mac can't play it; the make-local line (Queued to make local, Making local… with its `wait` and bytes, or Couldn't make local with its `error`); then the storage actions. Files waiting or moving, a pin running and a make-local show no actions; a pin still queued, a failed download and a copying release show theirs, Stop keeping here in place of Pin while copying.

**In the code:** macOS: `StorageStatusBand` (`Views/Library/AlbumDetail/StorageStatusBand.swift`), with `QueuedPinChips` and `UploadWaitChip`.
