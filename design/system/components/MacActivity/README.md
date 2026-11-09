# MacActivity

The toolbar's Activity panel, where bae lists what it needs the person for and what it's moving. Needs Attention comes first, each problem a quiet row of its box (no fill, the glyph in its tone), a line between rows: problems the caller passes in (children, each a `MacNotice`, such as a sign-in settings file that's missing or a library that wouldn't close), a store that failed (`error`, Retry Now its action) and sync's own (`attention`, a record and its reason, then Open Sync Settings). Then Transfers (syncing, uploads with bytes, downloads), or “Everything is up to date.” A problem shows here and nowhere else in the window; the toolbar's button shows the warning glyph while anything needs attention.

**In the code:** macOS: BaeKit's `SyncActivityList` in `SyncActivityButton`'s popover, which today holds only sync's own problems.
