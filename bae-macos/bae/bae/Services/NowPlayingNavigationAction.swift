import BaeKit
import os.log

private let logger = Logger.bae("NowPlayingNavigationAction")

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
            reportNoTarget()
            return
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

    /// Go to Now Playing ran while the state names no track in the library.
    /// Core's value that removed the track or stopped playback can arrive
    /// after the view drew the action enabled and before the person chose
    /// it. The action reports the event to the crash reporter as a bug, logs
    /// it, and tells the person something went wrong.
    private func reportNoTarget() {
        let detail =
            "Go to Now Playing ran with no library track in \(String(describing: playbackStore.nowPlaying))"
        logger.error("\(detail)")
        CoreBugReporter()
            .report(
                operation: "NowPlayingNavigationAction.perform",
                detail: detail
            )
        uiStore.showError(
            DisplayError(
                line: localizedCoreString("core.error.category.internal")
            )
        )
    }
}
