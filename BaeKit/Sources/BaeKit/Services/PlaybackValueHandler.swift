@MainActor
final class PlaybackValueHandler {
    private let playbackStore: PlaybackStore
    private let castStore: CastStore
    private var lastSeekRevision: UInt64 = 0
    /// The latest library position core sent. Positions arrive on their own
    /// subscription, so one can come before the value that shows its track;
    /// it is offered again once that track shows.
    private var latestPosition: BridgePlaybackPosition?

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

        let shownTrackId = playbackStore.nowPlaying.track?.track.trackId
        applyPlaybackState(values.state)
        if let latestPosition,
            let trackId = playbackStore.nowPlaying.track?.track.trackId,
            trackId != shownTrackId, latestPosition.trackId == trackId
        {
            playbackStore.updatePlaybackProgress(
                trackId: latestPosition.trackId,
                positionMs: latestPosition.positionMs,
                durationMs: latestPosition.durationMs,
                progress: latestPosition.progress
            )
        }
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

    func apply(_ positions: BridgePlaybackPositions) {
        let didSeek = positions.seekRevision != lastSeekRevision
        lastSeekRevision = positions.seekRevision
        latestPosition = positions.library
        guard let position = positions.library else {
            return
        }
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

    func applyQueueItemsAdded(_ count: UInt32) {
        playbackStore.publishQueueItemsAdded(Int(count))
    }

    func applyQueueSnapshot(_ snapshot: BridgeQueueSnapshot) {
        playbackStore.applyQueueSnapshot(snapshot)
    }
}
