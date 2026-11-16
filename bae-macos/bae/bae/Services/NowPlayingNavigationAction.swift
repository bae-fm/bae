import BaeKit

/// The library reveal shared by Command-L, the now-playing bar and the
/// queue's now-playing card: the playing track's row, on the release core
/// says it is on.
@MainActor
struct NowPlayingNavigationAction {
    let playbackStore: PlaybackStore
    let uiStore: UiStore

    /// Enabled while a track loads, plays or is paused and the library
    /// holds it; a removed track has no album to reveal.
    var isEnabled: Bool { target != nil }

    func perform() {
        guard case (let trackId, let display)? = target else {
            preconditionFailure(
                "Go to Now Playing is disabled without a track in the library"
            )
        }
        uiStore.navigateToAlbum(
            display.albumId,
            trackId: trackId,
            releaseId: display.releaseId
        )
    }

    /// The track the state names, with the library's display of it.
    private var target: (String, BridgeTrackDisplay)? {
        let state = playbackStore.nowPlaying
        guard let trackId = state.trackId,
            let display = state.display?.library
        else {
            return nil
        }
        return (trackId, display)
    }
}
