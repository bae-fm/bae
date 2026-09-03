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
        castStore.applyStatus(deviceName: values.remoteDeviceName)

        applyPlaybackState(values.state)
        applyPosition(values)
        lastSeekRevision = values.seekRevision
    }

    private func applyPlaybackState(_ state: BridgePlaybackValueState) {
        switch state {
        case .stopped:
            playbackStore.stop()
        case .loading(let trackId, let track):
            applyLoading(trackId: trackId, track: track)
        case .playing(
            let trackId,
            let trackTitle,
            let artistNames,
            _,
            let albumId,
            _,
            let coverImage,
            let durationMs
        ):
            applyPlaying(
                NowPlayingFields(
                    trackId: trackId,
                    trackTitle: trackTitle,
                    artistNames: artistNames,
                    albumId: albumId,
                    coverImage: coverImage,
                    durationMs: durationMs
                )
            )
        case .paused(
            let trackId,
            let trackTitle,
            let artistNames,
            _,
            let albumId,
            _,
            let coverImage,
            let durationMs,
            let reason
        ):
            let fields = NowPlayingFields(
                trackId: trackId,
                trackTitle: trackTitle,
                artistNames: artistNames,
                albumId: albumId,
                coverImage: coverImage,
                durationMs: durationMs
            )
            playbackStore.pause(track: fields.nowPlayingTrack(), reason: reason)
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

    private func applyPlaying(_ fields: NowPlayingFields) {
        playbackStore.play(track: fields.nowPlayingTrack())
    }

    private func applyLoading(
        trackId: String,
        track: BridgeLoadingTrackInfo?
    ) {
        if let track {
            playbackStore.setLoadingTarget(
                trackId: trackId,
                target: NowPlayingTrack(
                    trackId: trackId,
                    trackTitle: track.trackTitle,
                    artistNames: track.artistNames,
                    albumId: track.albumId,
                    coverImage: track.coverImage,
                    durationMs: track.durationMs
                )
            )
        }
        else {
            playbackStore.beginLoading(trackId: trackId)
        }
    }
}

private struct NowPlayingFields {
    let trackId: String
    let trackTitle: String
    let artistNames: String
    let albumId: String
    let coverImage: BridgeImageRef?
    let durationMs: UInt64

    func nowPlayingTrack() -> NowPlayingTrack {
        NowPlayingTrack(
            trackId: trackId,
            trackTitle: trackTitle,
            artistNames: artistNames,
            albumId: albumId,
            coverImage: coverImage,
            durationMs: durationMs
        )
    }
}
