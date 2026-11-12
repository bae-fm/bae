# MacNowPlayingBar

Along the bottom of the main window. Left: the track (opens its album). Centre: shuffle, previous, play/pause, next, repeat, and the seek bar. Right: cast, queue, mute, volume. `waiting` while the track is still arriving from the cloud: the arc turns around play/pause.

**In the code:** macOS: `MacNowPlayingBar` (`Views/Playback/NowPlaying/MacNowPlayingBar.swift`), with `MacNowPlayingBarContainer` reading the stores for it.
