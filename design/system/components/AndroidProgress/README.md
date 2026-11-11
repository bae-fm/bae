# AndroidProgress

The words say what is happening now, ending in an ellipsis: "Sharing the library…". Leave out `value` when the length isn't known.

**In the code:** Android: `StepProgressLine` (`ui/components/StepProgressLine.kt`, a step's title over a bar, its own `StepProgressView`), `DownloadProgress` (`ui/downloads/DownloadProgressView.kt`, a bar over its bytes) and `ShareBar` (`ui/sync/ShareBar.kt`); `StepBar` is used only by its own preview.
