# MacNowPlayingBar

Along the bottom of the main window. Left: the track (opens its album). Centre: shuffle, previous, play/pause, next, repeat, and the seek bar. Right: Play on, queue, mute, volume. `waiting` while the track is still arriving from the cloud: the arc turns around play/pause. `noVolume` (the receiver's name), playing on a receiver that takes no volume from bae, such as a UPnP renderer with no volume control: mute and the slider give way to "Volume is set on Living Room", so nothing offers a volume the receiver refuses.

The bar draws the track itself (the cover beside the title and artist), the transport with `MacTransport` and Mute and the slider, or the receiver's line, with `MacVolumeSlider`.

**In the code:** macOS: `MacNowPlayingBar` (`Views/Playback/NowPlaying/MacNowPlayingBar.swift`), with `MacNowPlayingBarContainer` reading the stores for it.
