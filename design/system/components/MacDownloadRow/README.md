# MacDownloadRow

A failed pin or make-local in the Storage Manager's inspector, built on `MacItem`: the pin (or folder) icon, the release's title with its files and size, the cancel mark; under them the failure, Couldn't pin or Couldn't make local, with Details, which opens the error in an alert, and Retry. The inspector shows it only when one has failed.

**In the code:** macOS: `DownloadRow` (`Views/Storage/Queue/DownloadRow.swift`), placed by `StorageTransferControls`.
