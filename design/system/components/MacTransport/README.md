# MacTransport

The now playing bar's transport: shuffle, previous, play and pause, next, and repeat.

| Prop | What it does |
|---|---|
| `playing` | The middle button shows Pause; otherwise Play. |
| `waiting` | The track waits on storage: an arc turns around the middle button. |
| `shuffle`, `repeat` | Each lights its button while on. |

**In the code:** macOS: none yet; `MacNowPlayingBar` (`Views/Playback/NowPlaying/MacNowPlayingBar.swift`) draws it inline.
