import BaeKit

/// The library reveal shared by Command-L, the now-playing bar and the
/// queue's now-playing card: the playing track's row, on the release core
/// says it is on.
@MainActor
struct NowPlayingNavigationAction {
    let playbackStore: PlaybackStore
    let uiStore: UiStore

    /// Enabled once core has said which track is current; while the next
    /// track is only an id, the bar still shows the one before it, and
    /// revealing that one would be revealing the wrong track.
    var isEnabled: Bool { playbackStore.nowPlaying.current != nil }

    func perform() {
        guard let track = playbackStore.nowPlaying.current else {
            preconditionFailure(
                "Go to Now Playing is disabled without a playing track"
            )
        }
        uiStore.navigateToAlbum(
            track.display.albumId,
            trackId: track.track.trackId,
            releaseId: track.display.releaseId
        )
    }
}
