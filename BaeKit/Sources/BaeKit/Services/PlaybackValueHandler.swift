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
        playbackStore.apply(values.state)
    }

    func applyQueueSnapshot(_ snapshot: BridgeQueueSnapshot) {
        playbackStore.applyQueueSnapshot(snapshot)
    }
}
