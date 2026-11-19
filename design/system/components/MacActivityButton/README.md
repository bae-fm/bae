# MacActivityButton

The title bar's Activity button, which opens the Activity panel. `state` says what the sync queue does:

- `moving`: a spinner, while bae moves files.
- `attention`: the warning mark, while something needs the person.
- `paused`: a pause symbol, while the queue is held (Paused, or Waiting for network) with nothing moving, so the person can see the hold.

With nothing moving or held, the title bar has no Activity button.

**In the code:** macOS: `ActivityButton` (`Views/Chrome/ActivityButton.swift`).
