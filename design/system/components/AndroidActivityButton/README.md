# AndroidActivityButton

The library header's Activity button, which opens Activity. `state` says what the sync queue does:

- `moving`: a spinner, while bae moves files.
- `attention`: the warning mark, while something needs the person.
- `paused`: a pause symbol, while the queue is held with nothing moving, so the person can see the hold.

With nothing moving or held, the header has no Activity button.

**In the code:** Android: `AndroidActivityButton` (`ui/sync/Activity.kt`), in the library's header (`ui/library/AndroidLibraryScreen.kt`).
