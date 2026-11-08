# MacDownloadRow

A failed download in the Storage Manager's inspector: the kind's icon, the release's title and its files and size, how long ago it was queued, the cancel mark; under them the failure box (Download failed for a pin, Couldn't make local for a make-local) with Retry as its action. The inspector shows it only when a download has failed.

**In the code:** macOS: `DownloadRow` (`Views/Storage/Queue/DownloadRow.swift`), placed by `StorageTransferControls`.
