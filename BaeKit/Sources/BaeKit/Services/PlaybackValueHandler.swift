@MainActor
final class PlaybackValueHandler {
    private let playbackStore: PlaybackStore
    private let castStore: CastStore

    init(
        playbackStore: PlaybackStore,
        castStore: CastStore
    ) {
        self.playbackStore = playbackStore
        self.castStore = castStore
    }

    func apply(_ values: BridgePlaybackValues) {
        playbackStore.settings = PlaybackSettings(values)
        playbackStore.failure = values.failure
        castStore.applyStatus(values.remote)
        applyPlaybackState(values.state)
    }

    private func applyPlaybackState(_ state: BridgePlaybackValueState) {
        switch state {
        case .stopped:
            playbackStore.stop()
        case .loading(.unprepared(let trackId), let waitsOnStorage):
            playbackStore.beginLoading(
                trackId: trackId,
                waitsOnStorage: waitsOnStorage
            )
        case .loading(.prepared(let track), let waitsOnStorage):
            playbackStore.setLoadingTarget(
                track,
                waitsOnStorage: waitsOnStorage
            )
        case .playing(let track):
            playbackStore.play(track: track)
        case .paused(let track, let reason):
            playbackStore.pause(track: track, reason: reason)
        }
    }

    func applyQueueSnapshot(_ snapshot: BridgeQueueSnapshot) {
        playbackStore.applyQueueSnapshot(snapshot)
    }
}
