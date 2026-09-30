@MainActor
final class PlaybackValueHandler {
    private let playbackStore: PlaybackStore
    private let castStore: CastStore
    private var lastSeekRevision: UInt64 = 0

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
        castStore.applyStatus(device: values.remoteDevice)

        applyPlaybackState(values.state)
        applyPosition(values)
        lastSeekRevision = values.seekRevision
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

    private func applyPosition(_ values: BridgePlaybackValues) {
        if let position = values.position {
            let didSeek = values.seekRevision != lastSeekRevision
            if didSeek {
                playbackStore.updatePlaybackSeeked(
                    trackId: position.trackId,
                    positionMs: position.positionMs,
                    durationMs: position.durationMs,
                    progress: position.progress
                )
            }
            else {
                playbackStore.updatePlaybackProgress(
                    trackId: position.trackId,
                    positionMs: position.positionMs,
                    durationMs: position.durationMs,
                    progress: position.progress
                )
            }
        }
    }

    func applyQueueItemsAdded(_ count: UInt32) {
        playbackStore.publishQueueItemsAdded(Int(count))
    }

    func applyQueueSnapshot(_ snapshot: BridgeQueueSnapshot) {
        playbackStore.applyQueueSnapshot(snapshot)
    }
}
