import AppKit
import os.log

private let logger = Logger.bae("UiEventReducer")

/// The stores and services a `UiEventReducer` writes, bundled so the reducer and
/// its per-category helpers take one dependency rather than nine separate ones.
@MainActor
struct ReducerContext {
    let playbackStore: PlaybackStore
    let configStore: ConfigStore
    let importStore: ImportStore
    let libraryStore: LibraryStore
    let appService: AppService
    let uiStore: UiStore
    let outboxStore: OutboxStore
    let downloadStore: DownloadStore
    let exportStore: ExportStore
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
    @MainActor
    static func reduce(_ event: BridgeUiEvent, into context: ReducerContext) {
        switch event {
        case .invalidated:
            break

        case .playbackPlaying, .playbackPaused, .playbackLoading,
            .playbackStopped, .playbackError, .playbackProgress,
            .playbackSeeked,
            .volumeChanged, .muteChanged, .repeatModeChanged,
            .queueUpdated, .queueItemsAdded:
            reducePlayback(event, into: context)

        case .previewPlaying, .previewPaused, .previewIdle, .previewProgress:
            reducePreview(event, into: context)

        case .candidateIdentifyStateChanged, .candidateSignalsUpdated,
            .candidateImportImporting, .candidateImportLoudnessProgress,
            .candidateImportComplete, .candidateImportError,
            .candidateSkipChanged:
            reduceCandidate(event, into: context)

        case .watchedFoldersChanged, .folderCandidateAdded, .invalidCandidate,
            .scanCandidateRemoved, .scanFinished:
            reduceScan(event, into: context)

        case .albumAdded, .albumUpdated, .albumRemoved,
            .releaseAdded, .releaseUpdated, .releaseRemoved,
            .releaseTransferProgress, .releaseTransferEnded:
            reduceLibrary(event, into: context)

        case .configChanged, .syncError, .syncTimeChanged, .syncingChanged,
            .outboxChanged, .downloadQueueChanged, .exportQueueChanged:
            reduceSyncAndConfig(event, into: context)

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
            .repeatModeChanged, .queueUpdated, .queueItemsAdded:
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

        case .queueUpdated(let snapshot):
            // The event carries display-ready items in two lanes (core resolves
            // them before emitting), so map them straight through — no DB
            // re-query here. The manual lane and the context render as distinct
            // sections.
            playbackStore.manualQueue = snapshot.manual.map(
                QueueItem.init(bridge:)
            )
            playbackStore.queueContext = snapshot.context.map(
                QueuePlaybackContext.init(bridge:)
            )
            context.appService.mediaControlService.updateCommandAvailability(
                hasNext: snapshot.hasNext,
                hasPrevious: snapshot.hasPrevious
            )

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
    private static func reduceCandidate(
        _ event: BridgeUiEvent,
        into context: ReducerContext
    ) {
        let importStore = context.importStore
        switch event {
        case .candidateIdentifyStateChanged(let key, let state, let toolbar):
            importStore.mutateCandidate(forKey: key) { candidate in
                candidate.identifyState = IdentifyState(bridge: state)
                candidate.signalsToolbar = SignalsToolbar(bridge: toolbar)
            }

        case .candidateSignalsUpdated(let key, let signals):
            importStore.mutateCandidate(forKey: key) {
                $0.signals = Signals(bridge: signals)
            }

        case .candidateImportImporting(
            let key,
            let progressPercent,
            let step
        ):
            importStore.mutateCandidate(forKey: key) {
                $0.importStatus = .importing(
                    progressPercent: progressPercent,
                    step: step
                )
            }

        case .candidateImportLoudnessProgress(
            let key,
            let tracksDone,
            let tracksTotal,
            let fraction
        ):
            // High-frequency sub-track tick — publish to the leaf bar's signal,
            // never the @Observable candidate row.
            importStore.importLoudnessSubject.send(
                ImportLoudnessProgressEvent(
                    key: key,
                    tracksDone: tracksDone,
                    tracksTotal: tracksTotal,
                    fraction: Double(fraction)
                )
            )

        case .candidateImportComplete(let key, let releaseId, let albumId):
            importStore.mutateCandidate(forKey: key) {
                $0.importStatus = .complete(
                    albumId: albumId,
                    releaseId: releaseId
                )
            }

        case .candidateImportError(let key, let error):
            importStore.mutateCandidate(forKey: key) {
                $0.importStatus = .error(error)
            }

        case .candidateSkipChanged(let key, let skipped):
            // The user skipped or unskipped the candidate; flip its flag so the
            // import view re-tabs it New ↔ Skipped. In-place mutation keeps the
            // candidate's identify/search/import state.
            importStore.mutateCandidate(forKey: key) { $0.skipped = skipped }

        default:
            break
        }
    }

    @MainActor
    private static func reduceScan(
        _ event: BridgeUiEvent,
        into context: ReducerContext
    ) {
        let importStore = context.importStore
        switch event {
        case .watchedFoldersChanged(let folders):
            // Pure assignment. Candidates of an unwatched folder aren't deleted
            // here — `candidateGroups` only surfaces candidates whose folder is
            // still watched, so a removed folder's rows simply stop rendering.
            importStore.watchedFolders = folders

        case .folderCandidateAdded(let candidate):
            // Insert only if absent: a re-scan (e.g. when the import view
            // reappears) re-emits every candidate, and overwriting would wipe a
            // candidate's in-progress identify/search/import state. The folder
            // may have been invalid before (a corrupt file got fixed), so drop
            // it from the invalid list — a folder is never in both.
            let native = Candidate(bridge: candidate)
            importStore.invalidCandidates.removeValue(forKey: native.key)
            if importStore.folderCandidates[native.key] == nil {
                importStore.folderCandidates[native.key] = native
            }

        case .invalidCandidate(let candidate):
            // A folder that looks like a release but failed validation. Surface
            // it under Skipped, and drop it from the valid-candidate list — it
            // may have been valid before (a file got corrupted); a folder is
            // never in both lists.
            importStore.invalidCandidates[candidate.folderPath] = candidate
            importStore.folderCandidates.removeValue(
                forKey: candidate.folderPath
            )

        case .scanCandidateRemoved(let key):
            // The watcher re-scanned the candidate's folder and it's gone from
            // disk; drop it from whichever list holds it.
            importStore.folderCandidates.removeValue(forKey: key)
            importStore.invalidCandidates.removeValue(forKey: key)

        case .scanFinished:
            // No state change needed — views react to candidate additions
            break

        default:
            break
        }
    }

    @MainActor
    private static func reduceLibrary(
        _ event: BridgeUiEvent,
        into context: ReducerContext
    ) {
        let libraryStore = context.libraryStore
        switch event {
        case .albumAdded(let album):
            logger.info(
                "reducer: albumAdded for album \(album.album.id)"
            )
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
            libraryStore.handleAlbumRemoved(
                albumId: albumId,
                releaseIds: releaseIds
            )
            context.importStore.handleAlbumRemoved(
                albumId: albumId,
                releaseIds: releaseIds
            )
            context.uiStore.clearSelectedRelease(inAlbum: albumId)
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
            context.importStore.handleReleaseRemoved(releaseId: releaseId)
            context.uiStore.clearSelectedReleaseIfMatching(
                releaseId,
                inAlbum: albumId
            )
            libraryStore.libraryShapeSubject.send(
                .releaseRemoved(albumId: albumId, releaseId: releaseId)
            )

        case .releaseTransferProgress, .releaseTransferEnded:
            reduceReleaseTransfer(event, into: context)

        default:
            break
        }
    }

    @MainActor
    private static func reduceReleaseTransfer(
        _ event: BridgeUiEvent,
        into context: ReducerContext
    ) {
        let libraryStore = context.libraryStore
        switch event {
        case .releaseTransferProgress(let releaseId, let action):
            libraryStore.handleReleaseTransferProgress(
                releaseId: releaseId,
                label: action.transferProgressVerb
            )

        case .releaseTransferEnded(let releaseId):
            libraryStore.handleReleaseTransferEnded(releaseId: releaseId)

        default:
            break
        }
    }

    @MainActor
    private static func reduceSyncAndConfig(
        _ event: BridgeUiEvent,
        into context: ReducerContext
    ) {
        let configStore = context.configStore
        switch event {
        case .configChanged(let config, let syncReady):
            configStore.config = Config(bridge: config)
            configStore.syncReady = syncReady

        case .syncError(let error):
            // `nil` error clears the banner (sync recovered). Otherwise show the
            // generic category line plus the opaque detail the error carries.
            configStore.syncError = error.map { DisplayError($0) }

        case .syncTimeChanged(let time):
            // `nil` time means no sync has completed yet — a real absence, so
            // the Date stays nil. Otherwise core sends epoch milliseconds.
            configStore.lastSyncTime = time.map {
                Date(timeIntervalSince1970: TimeInterval($0) / 1000)
            }

        case .syncingChanged(let syncing):
            configStore.syncing = syncing

        case .outboxChanged(let snapshot):
            context.outboxStore.snapshot = snapshot

        case .downloadQueueChanged(let snapshot):
            context.downloadStore.snapshot = snapshot

        case .exportQueueChanged(let snapshot):
            context.exportStore.snapshot = snapshot

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
