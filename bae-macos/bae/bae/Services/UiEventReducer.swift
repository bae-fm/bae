import AppKit
import BaeKit
import os.log

private let logger = Logger.bae("UiEventReducer")

/// The stores and services a `UiEventReducer` writes, bundled so the reducer and
/// its per-category helpers take one dependency.
@MainActor
struct ReducerContext {
    let playbackStore: PlaybackStore
    let importStore: ImportStore
    let projectionRegistry: ProjectionRegistry
    let appService: AppService
    let uiStore: UiStore
}

/// The fields carried by the `playbackPlaying` and `playbackPaused` events,
/// bundled so the two cases route to one `applyNowPlaying` helper.
private struct NowPlayingFields {
    let trackId: String
    let trackTitle: String
    let artistNames: String
    let artistId: String
    let albumId: String
    let albumTitle: String
    let coverImageId: String?
    let durationMs: UInt64

    /// Unpacks the track fields from a `playbackPlaying` or `playbackPaused`
    /// event; `nil` for any other variant.
    init?(event: BridgeUiEvent) {
        switch event {
        case .playbackPlaying(
            let trackId,
            let trackTitle,
            let artistNames,
            let artistId,
            let albumId,
            let albumTitle,
            let coverImageId,
            let durationMs
        ),
            .playbackPaused(
                let trackId,
                let trackTitle,
                let artistNames,
                let artistId,
                let albumId,
                let albumTitle,
                let coverImageId,
                let durationMs,
                _
            ):
            self.trackId = trackId
            self.trackTitle = trackTitle
            self.artistNames = artistNames
            self.artistId = artistId
            self.albumId = albumId
            self.albumTitle = albumTitle
            self.coverImageId = coverImageId
            self.durationMs = durationMs

        default:
            return nil
        }
    }

    /// The matching `BridgePlaybackState` for Control Center: `.playing` when
    /// `isPlaying`, otherwise `.paused`. Both constructors take these fields.
    func bridgeState(isPlaying: Bool) -> BridgePlaybackState {
        isPlaying
            ? .playing(
                trackId: trackId,
                trackTitle: trackTitle,
                artistNames: artistNames,
                artistId: artistId,
                albumId: albumId,
                albumTitle: albumTitle,
                coverImageId: coverImageId,
                durationMs: durationMs
            )
            : .paused(
                trackId: trackId,
                trackTitle: trackTitle,
                artistNames: artistNames,
                artistId: artistId,
                albumId: albumId,
                albumTitle: albumTitle,
                coverImageId: coverImageId,
                durationMs: durationMs
            )
    }

    func nowPlayingTrack() -> NowPlayingTrack {
        NowPlayingTrack(
            trackId: trackId,
            trackTitle: trackTitle,
            artistNames: artistNames,
            albumId: albumId,
            coverImageId: coverImageId,
            durationMs: durationMs
        )
    }
}

/// Reduces BridgeUiEvents into the appropriate stores. High-frequency
/// events go directly to NSViews via Combine subjects on AppService.
/// Everything else lands on the store that owns the field.
///
/// `reduce` groups the variants by concern and routes each group to a helper;
/// the helpers handle only the variants routed to them.
enum UiEventReducer {
    /// Builds the sink a `UiEventPump` drains on the main actor. The closure
    /// captures `appService` weakly: once the library is torn down (closed or
    /// switched) the reference is gone, so any event still queued is dropped
    /// with a warning instead of reduced into dead stores.
    @MainActor
    static func makeSink(
        appService: AppService
    ) -> @MainActor @Sendable (BridgeUiEvent) -> Void {
        { [weak appService] event in
            guard let appService else {
                logger.warning(
                    "Dropped UI event because the library event target was deallocated: \(String(describing: event))"
                )
                return
            }
            reduce(
                event,
                into: ReducerContext(
                    playbackStore: appService.playbackStore,
                    importStore: appService.importStore,
                    projectionRegistry: appService.projectionRegistry,
                    appService: appService,
                    uiStore: appService.uiStore
                )
            )
        }
    }

    @MainActor
    static func reduce(_ event: BridgeUiEvent, into context: ReducerContext) {
        switch event {
        case .invalidated(let invalidation):
            context.projectionRegistry.invalidate(invalidation)

        case .playbackPlaying, .playbackPaused, .playbackLoading,
            .playbackStopped, .playbackError, .playbackProgress,
            .playbackSeeked,
            .volumeChanged, .muteChanged, .repeatModeChanged,
            .queueItemsAdded:
            reducePlayback(event, into: context)

        case .previewPlaying, .previewPaused, .previewIdle, .previewProgress:
            reducePreview(event, into: context)

        case .candidateImportLoudnessProgress(
            let key,
            let tracksDone,
            let tracksTotal,
            let fraction
        ):
            context.importStore.importLoudnessSubject.send(
                ImportLoudnessProgressEvent(
                    key: key,
                    tracksDone: tracksDone,
                    tracksTotal: tracksTotal,
                    fraction: Double(fraction)
                )
            )

        case .queueUpdated(let snapshot):
            // Core resolves the full queue projection before emitting, so the
            // carried snapshot is the queue's update path — the same direct
            // apply every other platform does. The manual and context lanes
            // land on the store; `has_next`/`has_previous` drive Control
            // Center's next/previous availability.
            context.appService.applyQueueSnapshot(snapshot)

        case .releaseTransferProgress, .releaseTransferEnded:
            break

        case .error, .errorCleared:
            reduceErrors(event, into: context)
        }
    }

    // Each helper handles only the variants `reduce` routes to it; the trailing
    // `default` is unreachable in practice and present only for switch
    // exhaustiveness.

    @MainActor
    private static func reducePlayback(
        _ event: BridgeUiEvent,
        into context: ReducerContext
    ) {
        switch event {
        case .playbackPlaying, .playbackPaused, .playbackLoading,
            .playbackStopped:
            reducePlaybackNowPlaying(event, into: context)

        case .playbackError, .playbackProgress, .playbackSeeked,
            .volumeChanged, .muteChanged,
            .repeatModeChanged, .queueItemsAdded:
            reducePlaybackStateAndControls(event, into: context)

        default:
            break
        }
    }

    @MainActor
    private static func reducePlaybackNowPlaying(
        _ event: BridgeUiEvent,
        into context: ReducerContext
    ) {
        switch event {
        case .playbackPlaying:
            if let fields = NowPlayingFields(event: event) {
                applyNowPlaying(fields, into: context)
            }

        case .playbackPaused(
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            let reason
        ):
            if let fields = NowPlayingFields(event: event) {
                applyPausedNowPlaying(fields, reason: reason, into: context)
            }

        case .playbackLoading(let trackId, let track):
            applyLoading(trackId: trackId, track: track, into: context)

        case .playbackStopped:
            let playbackStore = context.playbackStore
            playbackStore.nowPlaying = .stopped
            playbackStore.resetPlaybackPosition()
            context.appService.mediaControlService.updateNowPlaying(
                state: .stopped,
                appHandle: context.appService.appHandle
            )

        default:
            break
        }
    }

    @MainActor
    private static func applyLoading(
        trackId: String,
        track: BridgeLoadingTrackInfo?,
        into context: ReducerContext
    ) {
        let playbackStore = context.playbackStore
        // Bare event (track == nil): enter loading, keep the prior track on
        // screen, and clear the frozen position bar. Detailed event: swap to
        // the resolved target so the bar updates while audio still loads.
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
        context.appService.mediaControlService.updateNowPlaying(
            state: .loading(trackId: trackId, track: track),
            appHandle: context.appService.appHandle
        )
    }

    /// Set the now-playing track (playing vs paused) on the store and mirror the
    /// matching `BridgePlaybackState` to Control Center. The store's
    /// `NowPlayingTrack` and Control Center's `BridgePlaybackState` carry the
    /// same fields; the latter also holds `artistId`/`albumTitle`, which
    /// `NowPlayingTrack` doesn't.
    @MainActor
    private static func applyNowPlaying(
        _ fields: NowPlayingFields,
        into context: ReducerContext
    ) {
        context.playbackStore.play(track: fields.nowPlayingTrack())
        updateMediaControls(fields, isPlaying: true, into: context)
    }

    @MainActor
    private static func applyPausedNowPlaying(
        _ fields: NowPlayingFields,
        reason: BridgePlaybackPauseReason,
        into context: ReducerContext
    ) {
        context.playbackStore.pause(
            track: fields.nowPlayingTrack(),
            reason: reason
        )
        updateMediaControls(fields, isPlaying: false, into: context)
    }

    @MainActor
    private static func updateMediaControls(
        _ fields: NowPlayingFields,
        isPlaying: Bool,
        into context: ReducerContext
    ) {
        context.appService.mediaControlService.updateNowPlaying(
            state: fields.bridgeState(isPlaying: isPlaying),
            appHandle: context.appService.appHandle
        )
    }

    @MainActor
    private static func reducePlaybackStateAndControls(
        _ event: BridgeUiEvent,
        into context: ReducerContext
    ) {
        let playbackStore = context.playbackStore
        switch event {
        case .playbackError(let reason):
            // A track couldn't be played (cloud-only not downloaded, decode
            // failure); core has already fallen back to stopped. Surface why —
            // the actionable cloud cases get their keyed line, everything else
            // the generic category line plus copyable detail.
            context.uiStore.showError(DisplayError(reason))

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

        case .volumeChanged(let volume):
            playbackStore.volume = volume

        case .muteChanged(let isMuted):
            playbackStore.isMuted = isMuted

        case .repeatModeChanged(let mode):
            playbackStore.repeatMode = RepeatMode(bridge: mode)

        case .queueItemsAdded(let count):
            playbackStore.queueItemsAddedSubject.send(Int(count))

        default:
            break
        }
    }

    @MainActor
    private static func updatePlaybackPosition(
        _ snapshot: PlaybackPositionSnapshot?,
        into context: ReducerContext
    ) {
        guard let snapshot else {
            return
        }
        context.appService.mediaControlService.updatePosition(
            positionMs: snapshot.positionMs,
            durationMs: snapshot.durationMs
        )
    }
}

extension UiEventReducer {
    @MainActor
    private static func reducePreview(
        _ event: BridgeUiEvent,
        into context: ReducerContext
    ) {
        let importStore = context.importStore
        switch event {
        case .previewPlaying(let path, let durationMs):
            importStore.previewState = .playing(
                path: path,
                durationMs: durationMs
            )
            context.appService.mediaControlService.updateNowPlayingForPreview(
                state: .playing(
                    path: path,
                    durationMs: durationMs
                )
            )

        case .previewPaused(let path, let durationMs):
            importStore.previewState = .paused(
                path: path,
                durationMs: durationMs
            )
            context.appService.mediaControlService.updateNowPlayingForPreview(
                state: .paused(
                    path: path,
                    durationMs: durationMs
                )
            )

        case .previewIdle:
            importStore.previewState = .idle
            importStore.previewProgressSubject.send(.reset)
            context.appService.mediaControlService.updateNowPlayingForPreview(
                state: .idle
            )

        case .previewProgress(let positionMs, let progress):
            importStore.previewProgressSubject.send(
                .position(
                    progress: progress,
                    elapsed: DurationClock.text(Int64(positionMs))
                )
            )
            context.appService.mediaControlService.updatePreviewPosition(
                positionMs: positionMs
            )

        default:
            break
        }
    }

    @MainActor
    private static func reduceErrors(
        _ event: BridgeUiEvent,
        into context: ReducerContext
    ) {
        switch event {
        case .error(let error):
            context.uiStore.showError(DisplayError(error))

        case .errorCleared:
            context.uiStore.clearError()

        default:
            break
        }
    }
}
