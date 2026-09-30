import BaeKit

/// The library reveal shared by Command-L and the queue's now-playing card:
/// the playing track's row, on the release core says it is on.
@MainActor
struct NowPlayingNavigationAction {
    let playbackStore: PlaybackStore
    let uiStore: UiStore

    var isEnabled: Bool { playbackStore.nowPlaying.track != nil }

    func perform() {
        guard let track = playbackStore.nowPlaying.track else {
            preconditionFailure(
                "Go to Now Playing is disabled without a playing track"
            )
        }
        uiStore.navigateToAlbum(
            track.albumId,
            trackId: track.trackId,
            releaseId: track.releaseId
        )
    }
}
