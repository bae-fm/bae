# AndroidActivity

The Activity sheet's list, opened from the library's top bar: the title, then Needs Attention, each problem a quiet row (no fill, the glyph in its tone) with a line between rows: (problems the caller passes in as children, each an `AndroidNotice`, a store that failed with Retry Now, and sync's own with Open Sync Settings), then the transfers (the downloads line opens Downloads), or up to date. Each problem has an × that dismisses it from the sheet; that doesn't fix it, and it still shows where it's fixed (such as Settings → Sync). A problem shows here and nowhere else on the screen; the top bar's button shows the warning glyph while a problem that hasn't been dismissed is listed; once every problem is dismissed, it goes back to its usual look. Put it in an `AndroidSheet`.

**In the code:** Android: `SyncActivityList` (`ui/sync/SyncActivity.kt`), which today holds only sync's own problems.
