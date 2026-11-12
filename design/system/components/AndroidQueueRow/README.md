# AndroidQueueRow

A queued track: cover, title, artists, album, duration, Remove, drag handle. `current` is the Now Playing row: accent title, no controls. `dragging` lifts it.

**In the code:** Android: `AndroidQueueRow` (`ui/playback/AndroidQueueRow.kt`), with `current` for the track playing. A row not loaded yet is `AndroidQueuePlaceholderRow`.
