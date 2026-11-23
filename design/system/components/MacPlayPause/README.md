# MacPlayPause

A 48pt circle on the tile color. `waiting`: an accent arc turns around it while the track arrives, and VoiceOver reads the arc as a progress indicator, Downloading the track…. The circle shows Pause while a track loads to play, so a bar that waits on its track passes `playing` too.

**In the code:** macOS: `MacPlayPause` (`Views/Playback/NowPlaying/MacNowPlayingBar.swift`): BaeKit's `PlayPauseControl` on a circle, with `StorageWaitArc` (`Views/Playback/NowPlaying/StorageWaitArc.swift`) turning round it while the track arrives.
