# AndroidNowPlayingBar

Along the bottom of every library screen: the track (opens the full player), previous, play/pause, next, cast (absent when casting is off), queue, repeat, then the seek bar. Hidden when nothing has played.

**In the code:** Android: `AndroidNowPlayingBar` (`ui/playback/AndroidNowPlayingBar.kt`), taking the track, a failed read and the transport, seek row and cast line as slots, with `NowPlayingTransport` beside it; `AndroidNowPlayingBarContainer` (`ui/playback/AndroidNowPlayingBarContainer.kt`) reads the session for it.
