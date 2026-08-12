@MainActor
final class PlaybackEventHandler {
    private let appHandle: AppHandle
    private let playbackStore: PlaybackStore
    private let castStore: CastStore
    private let mediaControlService: MediaControlService

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

    func apply(_ event: BridgeUiEvent) {
        switch event {
        case .playbackPlaying, .playbackPaused, .playbackLoading,
            .playbackStopped:
            applyNowPlayingEvent(event)

        case .playbackProgress, .playbackSeeked:
            applyPositionEvent(event)

        case .volumeChanged, .muteChanged, .repeatModeChanged,
            .queueItemsAdded:
            applyControlEvent(event)

        case .castStatusChanged(let deviceName):
            castStore.applyStatus(deviceName: deviceName)

        case .playbackError, .error, .previewPlaying, .previewPaused,
            .previewIdle, .previewProgress, .candidateImportLoudnessProgress,
            .importQueueIdentifyProgress:
            preconditionFailure("Unhandled playback event \(event)")
        }
    }

    private func applyNowPlayingEvent(_ event: BridgeUiEvent) {
        switch event {
        case .playbackPlaying:
            guard let fields = NowPlayingFields(event: event) else { return }
            applyNowPlaying(fields, isPlaying: true)

        case .playbackPaused(_, _, _, _, _, _, _, _, let reason):
            guard let fields = NowPlayingFields(event: event) else { return }
            playbackStore.pause(
                track: fields.nowPlayingTrack(),
                reason: reason
            )
            mediaControlService.updateNowPlaying(
                state: fields.bridgeState(isPlaying: false),
                appHandle: appHandle
            )

        case .playbackLoading(let trackId, let track):
            applyLoading(trackId: trackId, track: track)

        case .playbackStopped:
            playbackStore.nowPlaying = .stopped
            playbackStore.resetPlaybackPosition()
            mediaControlService.updateNowPlaying(
                state: .stopped,
                appHandle: appHandle
            )

        default:
            preconditionFailure("Unhandled Now Playing event \(event)")
        }
    }

    private func applyPositionEvent(_ event: BridgeUiEvent) {
        switch event {
        case .playbackProgress(
            let trackId,
            let positionMs,
            let durationMs,
            let progress
        ):
            updateMediaPosition(
                playbackStore.updatePlaybackProgress(
                    trackId: trackId,
                    positionMs: positionMs,
                    durationMs: durationMs,
                    progress: progress
                )
            )

        case .playbackSeeked(
            let trackId,
            let positionMs,
            let durationMs,
            let progress
        ):
            updateMediaPosition(
                playbackStore.updatePlaybackSeeked(
                    trackId: trackId,
                    positionMs: positionMs,
                    durationMs: durationMs,
                    progress: progress
                )
            )

        default:
            preconditionFailure("Unhandled playback position event \(event)")
        }
    }

    private func applyControlEvent(_ event: BridgeUiEvent) {
        switch event {
        case .volumeChanged(let volume):
            playbackStore.volume = volume
        case .muteChanged(let isMuted):
            playbackStore.isMuted = isMuted
        case .repeatModeChanged(let mode):
            playbackStore.repeatMode = mode
        case .queueItemsAdded(let count):
            playbackStore.publishQueueItemsAdded(Int(count))
        default:
            preconditionFailure("Unhandled playback control event \(event)")
        }
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

    init?(event: BridgeUiEvent) {
        switch event {
        case .playbackPlaying(
            let trackId,
            let trackTitle,
            let artistNames,
            let artistId,
            let albumId,
            let albumTitle,
            let coverImage,
            let durationMs
        ),
            .playbackPaused(
                let trackId,
                let trackTitle,
                let artistNames,
                let artistId,
                let albumId,
                let albumTitle,
                let coverImage,
                let durationMs,
                _
            ):
            self.trackId = trackId
            self.trackTitle = trackTitle
            self.artistNames = artistNames
            self.artistId = artistId
            self.albumId = albumId
            self.albumTitle = albumTitle
            self.coverImage = coverImage
            self.durationMs = durationMs

        default:
            return nil
        }
    }

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
