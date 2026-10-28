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
        playbackStore.volume = values.volume
        playbackStore.isMuted = values.isMuted
        playbackStore.repeatMode = values.repeatMode
        castStore.applyStatus(values.remote)
        applyPlaybackState(values.state)
    }

    private func applyPlaybackState(_ state: BridgePlaybackValueState) {
        switch state {
        case .stopped:
            playbackStore.stop()
        case .loading(.unprepared(let trackId)):
            playbackStore.beginLoading(trackId: trackId)
        case .loading(.prepared(let track)):
            playbackStore.setLoadingTarget(track)
        case .playing(let track):
            playbackStore.play(track: track)
        case .paused(let track, let reason):
            playbackStore.pause(track: track, reason: reason)
        }
    }

    func applyQueueItemsAdded(_ count: UInt32) {
        playbackStore.publishQueueItemsAdded(Int(count))
    }

    func applyQueueSnapshot(_ snapshot: BridgeQueueSnapshot) {
        playbackStore.applyQueueSnapshot(snapshot)
    }
}
