import os.log

private let logger = Logger.bae("UiEventReducer")

/// Receives UI events from core via the unified event stream and dispatches
/// them to the reducer on the main actor. Holds the stores and the
/// media-control service the reducer writes.
final class UiEventHandler: UiEventCallback, @unchecked Sendable {
    private let playbackStore: PlaybackStore
    private let configStore: ConfigStore
    private let libraryStore: LibraryStore
    private let downloadStore: DownloadStore
    private let mediaControlService: MediaControlService
    private weak var appHandle: AppHandle?

    init(
        playbackStore: PlaybackStore,
        configStore: ConfigStore,
        libraryStore: LibraryStore,
        downloadStore: DownloadStore,
        mediaControlService: MediaControlService,
        appHandle: AppHandle
    ) {
        self.playbackStore = playbackStore
        self.configStore = configStore
        self.libraryStore = libraryStore
        self.downloadStore = downloadStore
        self.mediaControlService = mediaControlService
        self.appHandle = appHandle
    }

    func onEvent(event: BridgeUiEvent) {
        guard let appHandle else {
            logger.debug(
                "dropping UI event after app handle released: \(String(describing: event))"
            )
            return
        }
        let playbackStore = playbackStore
        let configStore = configStore
        let libraryStore = libraryStore
        let downloadStore = downloadStore
        let mediaControlService = mediaControlService
        Task { @MainActor in
            let context = ReducerContext(
                playbackStore: playbackStore,
                configStore: configStore,
                libraryStore: libraryStore,
                downloadStore: downloadStore,
                mediaControlService: mediaControlService,
                appHandle: appHandle
            )
            UiEventReducer.reduce(event, into: context)
        }
    }
}

/// The stores and services a `UiEventReducer` writes, bundled so the reducer and
/// its per-category helpers take one dependency rather than five separate ones.
@MainActor
struct ReducerContext {
    let playbackStore: PlaybackStore
    let configStore: ConfigStore
    let libraryStore: LibraryStore
    let downloadStore: DownloadStore
    let mediaControlService: MediaControlService
    let appHandle: AppHandle
}

/// Reduces the mobile subset of `BridgeUiEvent`s into the shared stores and the
/// `MediaControlService`. bae-core owns playback on iOS, so playback / queue /
/// repeat / volume / mute variants drive the `PlaybackStore` (and the
/// lock-screen Now Playing info). Library, config, sync, and error events land
/// on their stores.
///
/// `reduce` groups the variants by store and routes each group to a helper;
/// the helpers handle only the variants routed to them. Desktop-only events
/// (import, scan, candidate, preview) never fire on iOS; the trailing `default`
/// logs any such variant so a future mobile-firing event is visible.
enum UiEventReducer {
    @MainActor
    static func reduce(_ event: BridgeUiEvent, into context: ReducerContext) {
        switch event {
        case .albumAdded, .albumUpdated, .albumRemoved,
            .releaseAdded, .releaseUpdated, .releaseRemoved:
            reduceLibraryShape(event, into: context)

        case .playbackPlaying, .playbackPaused:
            reducePlaybackTransport(event, into: context)

        case .playbackLoading, .playbackStopped, .playbackError,
            .playbackProgress, .playbackSeeked:
            reducePlaybackState(event, into: context)

        case .repeatModeChanged, .volumeChanged, .muteChanged, .queueUpdated:
            reducePlaybackControls(event, into: context)

        case .configChanged, .syncError, .error, .errorCleared:
            reduceConfigAndError(event, into: context)

        case .downloadQueueChanged(let snapshot):
            context.downloadStore.snapshot = snapshot

        default:
            logger.debug("ignoring event \(String(describing: event))")
        }
    }

    // Each helper handles only the variants `reduce` routes to it; the `default`
    // is unreachable in practice and present only for switch exhaustiveness.

    @MainActor
    private static func reduceLibraryShape(_ event: BridgeUiEvent, into context: ReducerContext) {
        let libraryStore = context.libraryStore
        switch event {
        case .albumAdded(let album):
            libraryStore.handleAlbumAdded(album: album)
            libraryStore.libraryShapeSubject.send(
                .albumAdded(albumId: album.album.id)
            )

        case .albumUpdated(let album):
            libraryStore.handleAlbumUpdated(album: album)
            libraryStore.libraryShapeSubject.send(
                .albumUpdated(albumId: album.album.id)
            )

        case .albumRemoved(let albumId, let releaseIds):
            libraryStore.handleAlbumRemoved(albumId: albumId, releaseIds: releaseIds)
            libraryStore.libraryShapeSubject.send(
                .albumRemoved(albumId: albumId)
            )

        case .releaseAdded(let album, let release):
            libraryStore.handleReleaseAdded(album: album, release: release)
            libraryStore.libraryShapeSubject.send(
                .releaseAdded(albumId: album.id, releaseId: release.id)
            )

        case .releaseUpdated(let albumId, let release):
            libraryStore.handleReleaseUpdated(release: release)
            libraryStore.libraryShapeSubject.send(
                .releaseUpdated(albumId: albumId, releaseId: release.id)
            )

        case .releaseRemoved(let albumId, let releaseId, let album):
            libraryStore.handleReleaseRemoved(
                releaseId: releaseId,
                album: album
            )
            libraryStore.libraryShapeSubject.send(
                .releaseRemoved(albumId: albumId, releaseId: releaseId)
            )

        default:
            break
        }
    }

    @MainActor
    private static func reducePlaybackTransport(
        _ event: BridgeUiEvent,
        into context: ReducerContext
    ) {
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
            applyNowPlaying(
                track: NowPlayingTrack(
                    trackId: trackId,
                    trackTitle: trackTitle,
                    artistNames: artistNames,
                    albumId: albumId,
                    coverImageId: coverImageId,
                    durationMs: durationMs
                ),
                albumTitle: albumTitle,
                into: context
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
            applyPausedNowPlaying(
                track: NowPlayingTrack(
                    trackId: trackId,
                    trackTitle: trackTitle,
                    artistNames: artistNames,
                    albumId: albumId,
                    coverImageId: coverImageId,
                    durationMs: durationMs
                ),
                albumTitle: albumTitle,
                reason: reason,
                into: context
            )

        default:
            break
        }
    }

    /// Set the now-playing track (playing vs paused) and mirror it to the lock
    /// screen, activating the audio session when a fresh playing event begins
    /// one. `albumTitle` is the one lock-screen field `NowPlayingTrack` doesn't
    /// carry (it holds `albumId` for navigation instead).
    @MainActor
    private static func applyNowPlaying(
        track: NowPlayingTrack,
        albumTitle: String,
        into context: ReducerContext
    ) {
        context.playbackStore.play(track: track)
        context.mediaControlService.beginPlaybackSession()
        updateMediaControls(
            track: track,
            albumTitle: albumTitle,
            isPlaying: true,
            into: context
        )
    }

    @MainActor
    private static func applyPausedNowPlaying(
        track: NowPlayingTrack,
        albumTitle: String,
        reason: BridgePlaybackPauseReason,
        into context: ReducerContext
    ) {
        context.playbackStore.pause(track: track, reason: reason)
        updateMediaControls(
            track: track,
            albumTitle: albumTitle,
            isPlaying: false,
            into: context
        )
    }

    @MainActor
    private static func reducePlaybackState(_ event: BridgeUiEvent, into context: ReducerContext) {
        let playbackStore = context.playbackStore
        let mediaControlService = context.mediaControlService
        switch event {
        case .playbackLoading(let trackId, let track):
            applyPlaybackLoading(trackId: trackId, track: track, into: context)
            mediaControlService.beginPlaybackSession()

        case .playbackStopped:
            playbackStore.nowPlaying = .stopped
            playbackStore.resetPlaybackPosition()
            mediaControlService.clearNowPlaying()
            mediaControlService.endPlaybackSession()

        case .playbackError(let reason):
            // A track couldn't be played (cloud-only not downloaded, decode
            // failure); core has already fallen back to stopped. Surface why —
            // the actionable cloud cases render their keyed line, everything
            // else the generic category line, resolved for the device locale.
            context.configStore.showError(DisplayError(reason))

        case .playbackProgress(
            let trackId,
            let positionMs,
            let durationMs,
            let progress
        ):
            updatePlaybackPosition(
                playbackStore.updatePlaybackProgress(
                    trackId: trackId,
                    positionMs: positionMs,
                    durationMs: durationMs,
                    progress: progress
                ),
                into: context
            )

        case .playbackSeeked(
            let trackId,
            let positionMs,
            let durationMs,
            let progress
        ):
            updatePlaybackPosition(
                playbackStore.updatePlaybackSeeked(
                    trackId: trackId,
                    positionMs: positionMs,
                    durationMs: durationMs,
                    progress: progress
                ),
                into: context
            )

        default:
            break
        }
    }

    @MainActor
    private static func reducePlaybackControls(_ event: BridgeUiEvent, into context: ReducerContext)
    {
        let playbackStore = context.playbackStore
        switch event {
        case .repeatModeChanged(let mode):
            playbackStore.repeatMode = RepeatMode(bridge: mode)

        case .volumeChanged(let volume):
            playbackStore.volume = volume

        case .muteChanged(let isMuted):
            playbackStore.isMuted = isMuted

        case .queueUpdated(let snapshot):
            playbackStore.applyQueueSnapshot(snapshot)
            context.mediaControlService.updateCommandAvailability(
                hasNext: snapshot.hasNext,
                hasPrevious: snapshot.hasPrevious
            )

        default:
            break
        }
    }

    @MainActor
    private static func reduceConfigAndError(_ event: BridgeUiEvent, into context: ReducerContext) {
        let configStore = context.configStore
        switch event {
        case .configChanged(let config, let syncReady):
            configStore.config = Config(bridge: config)
            configStore.syncReady = syncReady

        case .syncError(let error):
            // `nil` clears the banner (sync recovered). Otherwise render the
            // generic category line for the locale; the opaque detail rides
            // along on the `DisplayError` for a copyable disclosure.
            configStore.syncError = error.map { DisplayError($0) }

        case .error(let error):
            configStore.showError(DisplayError(error))

        case .errorCleared:
            configStore.clearError()

        default:
            break
        }
    }
}

@MainActor
private func applyPlaybackLoading(
    trackId: String,
    track: BridgeLoadingTrackInfo?,
    into context: ReducerContext
) {
    let playbackStore = context.playbackStore
    if let track {
        playbackStore.setLoadingTarget(
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
        playbackStore.beginLoading(trackId: trackId)
    }
}

@MainActor
private func updateMediaControls(
    track: NowPlayingTrack,
    albumTitle: String,
    isPlaying: Bool,
    into context: ReducerContext
) {
    context.mediaControlService.updateNowPlaying(
        NowPlayingMetadata(
            trackTitle: track.trackTitle,
            artistNames: track.artistNames,
            albumTitle: albumTitle,
            coverImageId: track.coverImageId,
            durationMs: track.durationMs
        ),
        isPlaying: isPlaying,
        appHandle: context.appHandle
    )
}

@MainActor
private func updatePlaybackPosition(
    _ snapshot: PlaybackPositionSnapshot?,
    into context: ReducerContext
) {
    guard let snapshot else {
        return
    }
    context.mediaControlService.updatePosition(
        positionMs: snapshot.positionMs,
        durationMs: snapshot.durationMs
    )
}
