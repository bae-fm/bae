import os.log

private let logger = Logger.bae("UiEventHandler")

/// Receives UI events from core and delivers them on the main actor in order.
final class UiEventHandler: UiEventCallback, @unchecked Sendable {
    private let eventContinuation: AsyncStream<BridgeUiEvent>.Continuation
    private let eventDeliveryTask: Task<Void, Never>

    init(appService: AppService) {
        let eventStream = AsyncStream.makeStream(
            of: BridgeUiEvent.self,
            bufferingPolicy: .unbounded
        )
        let deliveryTarget = UiEventDeliveryTarget(appService: appService)
        self.eventContinuation = eventStream.continuation
        self.eventDeliveryTask = Task { @MainActor in
            for await event in eventStream.stream {
                deliveryTarget.route(event)
            }
        }
    }

    func onEvent(event: BridgeUiEvent) {
        switch eventContinuation.yield(event) {
        case .enqueued:
            break

        case .dropped(let event):
            logger.warning(
                "Dropped UI event because the delivery stream buffer dropped it: \(String(describing: event))"
            )

        case .terminated:
            logger.warning(
                "Dropped UI event because the delivery stream has terminated: \(String(describing: event))"
            )

        @unknown default:
            logger.warning(
                "Dropped UI event because the delivery stream returned an unknown result: \(String(describing: event))"
            )
        }
    }

    deinit {
        eventContinuation.finish()
        eventDeliveryTask.cancel()
    }
}

private final class UiEventDeliveryTarget: @unchecked Sendable {
    private weak var appService: AppService?

    init(appService: AppService) {
        self.appService = appService
    }

    @MainActor
    func route(_ event: BridgeUiEvent) {
        guard let appService else {
            logger.warning(
                "Dropped UI event because the library event target was deallocated: \(String(describing: event))"
            )
            return
        }
        routeUiEvent(event, appService: appService)
    }
}

@MainActor
private func routeUiEvent(_ event: BridgeUiEvent, appService: AppService) {
    if routeInvalidation(event, appService: appService) {
        return
    }
    if let event = PlaybackTransportEvent(event) {
        routePlaybackTransport(event, appService: appService)
        return
    }
    if let event = PlaybackStateEvent(event) {
        routePlaybackState(event, appService: appService)
        return
    }
    if let event = PlaybackControlEvent(event) {
        routePlaybackControls(event, appService: appService)
        return
    }
    if routeErrors(event, appService: appService) {
        return
    }
    ignoreObsoleteOrDesktopEvent(event)
}

@MainActor
private func routeInvalidation(
    _ event: BridgeUiEvent,
    appService: AppService
) -> Bool {
    switch event {
    case .invalidated(let invalidation):
        appService.projectionRegistry.invalidate(invalidation)
        return true

    default:
        return false
    }
}

@MainActor
private func routePlaybackTransport(
    _ event: PlaybackTransportEvent,
    appService: AppService
) {
    switch event {
    case .playing(let update):
        appService.playbackStore.play(track: update.track)
        appService.mediaControlService.beginPlaybackSession()
        updateMediaControls(
            update,
            isPlaying: true,
            appService: appService
        )

    case .paused(let update, let reason):
        appService.playbackStore.pause(
            track: update.track,
            reason: reason
        )
        updateMediaControls(
            update,
            isPlaying: false,
            appService: appService
        )

    case .loading(let trackId, let track):
        applyPlaybackLoading(
            trackId: trackId,
            track: track,
            appService: appService
        )
        appService.mediaControlService.beginPlaybackSession()

    case .stopped:
        appService.playbackStore.nowPlaying = .stopped
        appService.playbackStore.resetPlaybackPosition()
        appService.mediaControlService.clearNowPlaying()
        appService.mediaControlService.endPlaybackSession()
    }
}

@MainActor
private func routePlaybackState(
    _ event: PlaybackStateEvent,
    appService: AppService
) {
    switch event {
    case .error(let reason):
        appService.configStore.showError(DisplayError(reason))

    case .progress(
        let trackId,
        let positionMs,
        let durationMs,
        let progress
    ):
        updatePlaybackPosition(
            appService.playbackStore.updatePlaybackProgress(
                trackId: trackId,
                positionMs: positionMs,
                durationMs: durationMs,
                progress: progress
            ),
            appService: appService
        )

    case .seeked(
        let trackId,
        let positionMs,
        let durationMs,
        let progress
    ):
        updatePlaybackPosition(
            appService.playbackStore.updatePlaybackSeeked(
                trackId: trackId,
                positionMs: positionMs,
                durationMs: durationMs,
                progress: progress
            ),
            appService: appService
        )
    }
}

@MainActor
private func routePlaybackControls(
    _ event: PlaybackControlEvent,
    appService: AppService
) {
    switch event {
    case .volume(let volume):
        appService.playbackStore.volume = volume

    case .mute(let isMuted):
        appService.playbackStore.isMuted = isMuted

    case .repeatMode(let mode):
        appService.playbackStore.repeatMode = RepeatMode(bridge: mode)

    case .queue(let snapshot):
        appService.applyQueueSnapshot(snapshot)

    case .queueItemsAdded(let count):
        appService.playbackStore.queueItemsAddedSubject.send(Int(count))
    }
}

@MainActor
private func routeErrors(
    _ event: BridgeUiEvent,
    appService: AppService
) -> Bool {
    switch event {
    case .error(let error):
        appService.configStore.showError(DisplayError(error))
        return true

    case .errorCleared:
        appService.configStore.clearError()
        return true

    default:
        return false
    }
}

@MainActor
private func ignoreObsoleteOrDesktopEvent(_ event: BridgeUiEvent) {
    switch event {
    case .previewIdle, .previewPlaying, .previewPaused, .previewProgress,
        .candidateIdentifyStateChanged, .candidateSignalsUpdated,
        .candidateImportImporting, .candidateImportLoudnessProgress,
        .candidateImportComplete, .candidateImportError,
        .watchedFoldersChanged, .folderCandidateAdded, .invalidCandidate,
        .scanCandidateRemoved, .candidateSkipChanged, .scanFinished,
        .albumAdded, .albumUpdated, .albumRemoved, .releaseAdded,
        .releaseUpdated, .releaseRemoved, .configChanged, .syncError,
        .syncTimeChanged, .syncingChanged, .outboxChanged,
        .releaseTransferProgress, .releaseTransferEnded,
        .downloadQueueChanged, .exportQueueChanged:
        break

    case .invalidated, .playbackStopped, .playbackError, .playbackLoading,
        .playbackPlaying, .playbackPaused, .playbackProgress,
        .playbackSeeked, .volumeChanged, .muteChanged, .repeatModeChanged,
        .queueUpdated, .queueItemsAdded, .error, .errorCleared:
        preconditionFailure("Unhandled UI event \(event)")
    }
}

@MainActor
private func applyPlaybackLoading(
    trackId: String,
    track: BridgeLoadingTrackInfo?,
    appService: AppService
) {
    if let track {
        appService.playbackStore.setLoadingTarget(
            trackId: trackId,
            target: NowPlayingTrack(
                trackId: trackId,
                trackTitle: track.trackTitle,
                artistNames: track.artistNames,
                albumId: track.albumId,
                coverImageId: track.coverImageId,
                durationMs: track.durationMs
            )
        )
    }
    else {
        appService.playbackStore.beginLoading(trackId: trackId)
    }
}

@MainActor
private func updateMediaControls(
    _ update: PlaybackNowPlayingUpdate,
    isPlaying: Bool,
    appService: AppService
) {
    appService.mediaControlService.updateNowPlaying(
        update.metadata(),
        isPlaying: isPlaying,
        appHandle: appService.appHandle
    )
}

@MainActor
private func updatePlaybackPosition(
    _ snapshot: PlaybackPositionSnapshot?,
    appService: AppService
) {
    guard let snapshot else {
        return
    }
    appService.mediaControlService.updatePosition(
        positionMs: snapshot.positionMs,
        durationMs: snapshot.durationMs
    )
}

private enum PlaybackTransportEvent {
    case playing(PlaybackNowPlayingUpdate)
    case paused(PlaybackNowPlayingUpdate, BridgePlaybackPauseReason)
    case loading(trackId: String, track: BridgeLoadingTrackInfo?)
    case stopped

    init?(_ event: BridgeUiEvent) {
        switch event {
        case .playbackPlaying(
            let trackId,
            let trackTitle,
            let artistNames,
            _,
            let albumId,
            let albumTitle,
            let coverImageId,
            let durationMs
        ):
            self = .playing(
                PlaybackNowPlayingUpdate(
                    track: NowPlayingTrack(
                        trackId: trackId,
                        trackTitle: trackTitle,
                        artistNames: artistNames,
                        albumId: albumId,
                        coverImageId: coverImageId,
                        durationMs: durationMs
                    ),
                    albumTitle: albumTitle
                )
            )

        case .playbackPaused(
            let trackId,
            let trackTitle,
            let artistNames,
            _,
            let albumId,
            let albumTitle,
            let coverImageId,
            let durationMs,
            let reason
        ):
            self = .paused(
                PlaybackNowPlayingUpdate(
                    track: NowPlayingTrack(
                        trackId: trackId,
                        trackTitle: trackTitle,
                        artistNames: artistNames,
                        albumId: albumId,
                        coverImageId: coverImageId,
                        durationMs: durationMs
                    ),
                    albumTitle: albumTitle
                ),
                reason
            )

        case .playbackLoading(let trackId, let track):
            self = .loading(trackId: trackId, track: track)

        case .playbackStopped:
            self = .stopped

        default:
            return nil
        }
    }
}

private enum PlaybackStateEvent {
    case error(BridgePlaybackErrorReason)
    case progress(
        trackId: String,
        positionMs: UInt64,
        durationMs: UInt64,
        progress: Double
    )
    case seeked(
        trackId: String,
        positionMs: UInt64,
        durationMs: UInt64,
        progress: Double
    )

    init?(_ event: BridgeUiEvent) {
        switch event {
        case .playbackError(let reason):
            self = .error(reason)

        case .playbackProgress(
            let trackId,
            let positionMs,
            let durationMs,
            let progress
        ):
            self = .progress(
                trackId: trackId,
                positionMs: positionMs,
                durationMs: durationMs,
                progress: progress
            )

        case .playbackSeeked(
            let trackId,
            let positionMs,
            let durationMs,
            let progress
        ):
            self = .seeked(
                trackId: trackId,
                positionMs: positionMs,
                durationMs: durationMs,
                progress: progress
            )

        default:
            return nil
        }
    }
}

private enum PlaybackControlEvent {
    case volume(Float)
    case mute(Bool)
    case repeatMode(BridgeRepeatMode)
    case queue(BridgeQueueSnapshot)
    case queueItemsAdded(UInt32)

    init?(_ event: BridgeUiEvent) {
        switch event {
        case .volumeChanged(let volume):
            self = .volume(volume)

        case .muteChanged(let isMuted):
            self = .mute(isMuted)

        case .repeatModeChanged(let mode):
            self = .repeatMode(mode)

        case .queueUpdated(let snapshot):
            self = .queue(snapshot)

        case .queueItemsAdded(let count):
            self = .queueItemsAdded(count)

        default:
            return nil
        }
    }
}

private struct PlaybackNowPlayingUpdate {
    let track: NowPlayingTrack
    let albumTitle: String

    func metadata() -> NowPlayingMetadata {
        NowPlayingMetadata(
            trackTitle: track.trackTitle,
            artistNames: track.artistNames,
            albumTitle: albumTitle,
            coverImageId: track.coverImageId,
            durationMs: track.durationMs
        )
    }
}
