# AndroidActivity

The Activity sheet's list, opened from the library's top bar: the title, then Needs Attention as notices (problems the caller passes in as children, each an `AndroidNotice`, a store that failed with Retry Now, and sync's own with Open Sync Settings), then the transfers (the downloads line opens Downloads), or up to date. A problem shows here and nowhere else on the screen; the top bar's button shows the warning glyph while anything needs attention. Put it in an `AndroidSheet`.

**In the code:** Android: `SyncActivityList` (`ui/sync/SyncActivity.kt`), which today holds only sync's own problems.
