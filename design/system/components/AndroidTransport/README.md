# AndroidTransport

Previous, play and pause, and next, in the now playing bar and the full player.

| Prop | What it does |
|---|---|
| `playing` | The middle button shows Pause; otherwise Play. |
| `waiting` | The track waits on storage: an arc turns around the middle button, which shows Pause. |
| `large` | The full player's: bigger buttons, spread across the sheet. |

**In the code:** Android: `NowPlayingTransport` (`ui/playback/AndroidNowPlayingBar.kt`) and `ExpandedTransportRow` (`ui/playback/AndroidFullPlayer.kt`).
