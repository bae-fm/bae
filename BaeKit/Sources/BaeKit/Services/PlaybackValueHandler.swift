@MainActor
final class PlaybackValueHandler {
    private let playbackStore: PlaybackStore
    private let castStore: CastStore
    /// The number of the last failure shown, so a value that reads it again
    /// shows nothing.
    private var shownFailure: UInt64?

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

    /// Why playback failed, the first time a value carries that failure.
    func failureToShow(
        _ values: BridgePlaybackValues
    ) -> BridgePlaybackErrorReason? {
        guard let failure = values.failure, failure.number != shownFailure
        else { return nil }
        shownFailure = failure.number
        return failure.reason
    }

    func applyQueueItemsAdded(_ count: UInt32) {
        playbackStore.publishQueueItemsAdded(Int(count))
    }

    func applyQueueSnapshot(_ snapshot: BridgeQueueSnapshot) {
        playbackStore.applyQueueSnapshot(snapshot)
    }
}
