@MainActor
final class PlaybackEventHandler {
    private let appHandle: AppHandle
    private let playbackStore: PlaybackStore
    private let castStore: CastStore
    private let mediaControlService: MediaControlService
    private var lastSeekRevision: UInt64 = 0

    init(
        appHandle: AppHandle,
        playbackStore: PlaybackStore,
        castStore: CastStore,
        mediaControlService: MediaControlService
    ) {
        self.appHandle = appHandle
        self.playbackStore = playbackStore
        self.castStore = castStore
        self.mediaControlService = mediaControlService
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
            playbackStore.nowPlaying = .stopped
            playbackStore.resetPlaybackPosition()
            mediaControlService.updateNowPlaying(
                state: .stopped,
                appHandle: appHandle
            )
        case .loading(let trackId, let track):
            applyLoading(trackId: trackId, track: track)
        case .playing(
            let trackId,
            let trackTitle,
            let artistNames,
            let artistId,
            let albumId,
            let albumTitle,
            let coverImage,
            let durationMs
        ):
            applyNowPlaying(
                NowPlayingFields(
                    trackId: trackId,
                    trackTitle: trackTitle,
                    artistNames: artistNames,
                    artistId: artistId,
                    albumId: albumId,
                    albumTitle: albumTitle,
                    coverImage: coverImage,
                    durationMs: durationMs
                ),
                isPlaying: true
            )
        case .paused(
            let trackId,
            let trackTitle,
            let artistNames,
            let artistId,
            let albumId,
            let albumTitle,
            let coverImage,
            let durationMs,
            let reason
        ):
            let fields = NowPlayingFields(
                trackId: trackId,
                trackTitle: trackTitle,
                artistNames: artistNames,
                artistId: artistId,
                albumId: albumId,
                albumTitle: albumTitle,
                coverImage: coverImage,
                durationMs: durationMs
            )
            playbackStore.pause(track: fields.nowPlayingTrack(), reason: reason)
            mediaControlService.updateNowPlaying(
                state: fields.bridgeState(isPlaying: false),
                appHandle: appHandle
            )
        }
    }

    private func applyPosition(_ values: BridgePlaybackValues) {
        if let position = values.position {
            let didSeek = values.seekRevision != lastSeekRevision
            updateMediaPosition(
                didSeek
                    ? playbackStore.updatePlaybackSeeked(
                        trackId: position.trackId,
                        positionMs: position.positionMs,
                        durationMs: position.durationMs,
                        progress: position.progress
                    )
                    : playbackStore.updatePlaybackProgress(
                        trackId: position.trackId,
                        positionMs: position.positionMs,
                        durationMs: position.durationMs,
                        progress: position.progress
                    )
            )
        }
    }

    func applyQueueItemsAdded(_ count: UInt32) {
        playbackStore.publishQueueItemsAdded(Int(count))
    }

    func applyQueueSnapshot(_ snapshot: BridgeQueueSnapshot) {
        playbackStore.applyQueueSnapshot(snapshot)
        mediaControlService.updateCommandAvailability(
            hasNext: snapshot.hasNext,
            hasPrevious: snapshot.hasPrevious
        )
    }

    private func applyNowPlaying(_ fields: NowPlayingFields, isPlaying: Bool) {
        playbackStore.play(track: fields.nowPlayingTrack())
        mediaControlService.updateNowPlaying(
            state: fields.bridgeState(isPlaying: isPlaying),
            appHandle: appHandle
        )
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
        mediaControlService.updateNowPlaying(
            state: .loading(trackId: trackId, track: track),
            appHandle: appHandle
        )
    }

    private func updateMediaPosition(_ snapshot: PlaybackPositionSnapshot?) {
        guard let snapshot else { return }
        mediaControlService.updatePosition(
            positionMs: snapshot.positionMs,
            durationMs: snapshot.durationMs
        )
    }
}

private struct NowPlayingFields {
    let trackId: String
    let trackTitle: String
    let artistNames: String
    let artistId: String
    let albumId: String
    let albumTitle: String
    let coverImage: BridgeImageRef?
    let durationMs: UInt64

    func bridgeState(isPlaying: Bool) -> BridgePlaybackState {
        isPlaying
            ? .playing(
                trackId: trackId,
                trackTitle: trackTitle,
                artistNames: artistNames,
                artistId: artistId,
                albumId: albumId,
                albumTitle: albumTitle,
                coverImage: coverImage,
                durationMs: durationMs
            )
            : .paused(
                trackId: trackId,
                trackTitle: trackTitle,
                artistNames: artistNames,
                artistId: artistId,
                albumId: albumId,
                albumTitle: albumTitle,
                coverImage: coverImage,
                durationMs: durationMs
            )
    }

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
