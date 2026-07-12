import OSLog

private let logger = Logger.bae("UiEventDispatcher")

/// Dispatches a `BridgeUiEvent` off `UiEventPump` into the shared stores and
/// services every Apple platform writes: `PlaybackStore`, `MediaControlService`,
/// `AppService`, `ProjectionRegistry`. Most variants apply identically on every
/// platform — `dispatch` handles exactly those and reports whether it did.
/// Variants with no shared handling (preview, import-loudness) resolve to
/// `.unhandled`; the platform sink's `onUnhandled` owns what happens to them.
public enum UiEventDispatcher {
    /// Whether a shared arm consumed the event. The platform sink decides what
    /// to do with `.unhandled` events: macOS handles its desktop-only events
    /// and drops the rest; iOS ignores the known desktop-only events and traps
    /// on anything else.
    public enum Outcome {
        case handled
        case unhandled
    }

    /// One exhaustive switch over every `BridgeUiEvent` variant, with no
    /// `default` — a new variant fails the BaeKit build until it's routed here
    /// explicitly. Groups variants by concern and routes each group to a
    /// helper, so no single switch grows past the lint complexity threshold.
    @MainActor
    public static func dispatch(
        _ event: BridgeUiEvent,
        appService: AppService
    ) -> Outcome {
        switch event {
        case .invalidated(let invalidation):
            appService.projectionRegistry.invalidate(invalidation)

        case .playbackPlaying, .playbackPaused, .playbackLoading,
            .playbackStopped, .playbackError, .playbackProgress,
            .playbackSeeked, .volumeChanged, .muteChanged,
            .repeatModeChanged, .queueItemsAdded:
            dispatchPlayback(event, appService: appService)

        case .queueUpdated(let snapshot):
            // Core resolves the full queue projection before emitting, so the
            // carried snapshot is the queue's update path — direct apply, the
            // same as every other platform. `hasNext`/`hasPrevious` drive
            // Control Center's next/previous availability.
            appService.applyQueueSnapshot(snapshot)

        case .error(let error):
            appService.showError(DisplayError(error))

        case .releaseTransferProgress, .releaseTransferEnded:
            // Neither Apple app renders a per-transfer indicator today; consume
            // the event here rather than leaving it for the platform tail.
            break

        case .previewPlaying, .previewPaused, .previewIdle, .previewProgress,
            .candidateImportLoudnessProgress:
            return .unhandled
        }
        return .handled
    }

    // `dispatchPlayback` and its helpers handle only the variants routed to
    // them by `dispatch`; the trailing `default` in each is unreachable in
    // practice and present only for switch exhaustiveness.

    @MainActor
    private static func dispatchPlayback(
        _ event: BridgeUiEvent,
        appService: AppService
    ) {
        switch event {
        case .playbackPlaying, .playbackPaused, .playbackLoading,
            .playbackStopped:
            dispatchNowPlaying(event, appService: appService)

        case .playbackError, .playbackProgress, .playbackSeeked,
            .volumeChanged, .muteChanged, .repeatModeChanged,
            .queueItemsAdded:
            dispatchPlaybackStateAndControls(event, appService: appService)

        default:
            break
        }
    }

    @MainActor
    private static func dispatchNowPlaying(
        _ event: BridgeUiEvent,
        appService: AppService
    ) {
        switch event {
        case .playbackPlaying:
            if let fields = NowPlayingFields(event: event) {
                applyNowPlaying(fields, appService: appService)
            }

        case .playbackPaused(_, _, _, _, _, _, _, _, let reason):
            if let fields = NowPlayingFields(event: event) {
                applyPausedNowPlaying(
                    fields,
                    reason: reason,
                    appService: appService
                )
            }

        case .playbackLoading(let trackId, let track):
            applyLoading(trackId: trackId, track: track, appService: appService)

        case .playbackStopped:
            appService.playbackStore.nowPlaying = .stopped
            appService.playbackStore.resetPlaybackPosition()
            appService.mediaControlService.updateNowPlaying(
                state: .stopped,
                appHandle: appService.appHandle
            )

        default:
            break
        }
    }

    @MainActor
    private static func dispatchPlaybackStateAndControls(
        _ event: BridgeUiEvent,
        appService: AppService
    ) {
        let playbackStore = appService.playbackStore
        switch event {
        case .playbackError(let reason):
            // A track couldn't be played (cloud-only not downloaded, decode
            // failure); core has already fallen back to stopped. Surface why —
            // the actionable cloud cases get their keyed line, everything else
            // the generic category line plus copyable detail.
            appService.showError(DisplayError(reason))

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
                appService: appService
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
                appService: appService
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

    /// Builds the sink a `UiEventPump` drains on the main actor: weak
    /// `appService` capture with drop-with-warning after library teardown,
    /// shared `dispatch`, then the platform's `onUnhandled` for events no
    /// shared arm consumes. Generic over the platform subclass so
    /// `onUnhandled` receives the concrete type without a downcast.
    @MainActor
    public static func makeSink<Service: AppService>(
        appService: Service,
        onUnhandled:
            @escaping @MainActor @Sendable (BridgeUiEvent, Service) ->
            Void
    ) -> @MainActor @Sendable (BridgeUiEvent) -> Void {
        { [weak appService] event in
            guard let appService else {
                logger.warning(
                    "Dropped UI event because the library event target was deallocated: \(String(describing: event))"
                )
                return
            }
            if dispatch(event, appService: appService) == .unhandled {
                onUnhandled(event, appService)
            }
        }
    }
}

extension UiEventDispatcher {
    @MainActor
    private static func applyNowPlaying(
        _ fields: NowPlayingFields,
        appService: AppService
    ) {
        appService.playbackStore.play(track: fields.nowPlayingTrack())
        appService.mediaControlService.updateNowPlaying(
            state: fields.bridgeState(isPlaying: true),
            appHandle: appService.appHandle
        )
    }

    @MainActor
    private static func applyPausedNowPlaying(
        _ fields: NowPlayingFields,
        reason: BridgePlaybackPauseReason,
        appService: AppService
    ) {
        appService.playbackStore.pause(
            track: fields.nowPlayingTrack(),
            reason: reason
        )
        appService.mediaControlService.updateNowPlaying(
            state: fields.bridgeState(isPlaying: false),
            appHandle: appService.appHandle
        )
    }

    @MainActor
    private static func applyLoading(
        trackId: String,
        track: BridgeLoadingTrackInfo?,
        appService: AppService
    ) {
        let playbackStore = appService.playbackStore
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
        appService.mediaControlService.updateNowPlaying(
            state: .loading(trackId: trackId, track: track),
            appHandle: appService.appHandle
        )
    }

    @MainActor
    private static func updatePlaybackPosition(
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
}

/// The fields carried by the `playbackPlaying` and `playbackPaused` events,
/// bundled so the two cases route to one `applyNowPlaying` helper.
struct NowPlayingFields {
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
