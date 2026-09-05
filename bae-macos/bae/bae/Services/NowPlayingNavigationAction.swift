import BaeKit

/// The library reveal shared by Command-L and the queue's now-playing card.
@MainActor
struct NowPlayingNavigationAction {
    let playbackStore: PlaybackStore
    let libraryStore: LibraryStore
    let uiStore: UiStore

    var isEnabled: Bool { playbackStore.nowPlaying.track?.albumId != nil }

    func perform() {
        guard let albumId = playbackStore.nowPlaying.track?.albumId
        else {
            preconditionFailure(
                "Go to Now Playing is disabled without a playing album"
            )
        }
        let trackId = playbackStore.nowPlaying.track?.trackId
        // Store an override only when the playing track's release is not the
        // album default; unloaded details leave the default unchanged.
        let releaseId: String? = {
            guard let trackId,
                let summary = libraryStore.albumSummaries[albumId]
            else {
                return nil
            }
            let matchingReleaseId = summary.releaseIds.first { id in
                libraryStore.releaseDetails[id]?.tracks
                    .contains(where: { $0.id == trackId }) ?? false
            }
            guard let matchingReleaseId else {
                return nil
            }
            return matchingReleaseId == summary.primaryReleaseId
                ? nil : matchingReleaseId
        }()
        uiStore.navigateToAlbum(
            albumId,
            trackId: trackId,
            releaseId: releaseId
        )
    }
}
