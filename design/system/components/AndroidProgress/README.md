# AndroidProgress

The words say what is happening now, ending in an ellipsis: "Sharing the library…". Leave out `value` when the length isn't known. The words also name the bar for VoiceOver and TalkBack, which read a bar by its name; a bar whose words sit elsewhere (a pin's bytes beside it) takes `name` instead, which names it without showing. Every bar has a name.

**In the code:** Android: `AndroidProgress` (`ui/components/AndroidProgress.kt`, a step's title over a bar, its own `StepProgressView`), used by `OpenProgressLine`; `DownloadProgress` (`ui/downloads/DownloadProgressView.kt`, a bar over its bytes) and `ShareBar` (`ui/sync/ShareBar.kt`) are the other two.
