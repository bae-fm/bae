# AndroidQueueRow

A queued track: cover, title, artists, album, duration, Remove, drag handle. `current` is the Now playing row: accent title, no controls. `dragging` lifts it: the held row follows the finger with a shadow, and the other rows slide apart, leaving an open slot where it will land.

**In the code:** Android: `AndroidQueueRow` (`ui/playback/AndroidQueueRow.kt`), with `current` for the track playing. A row not loaded yet is `AndroidQueuePlaceholderRow`.
