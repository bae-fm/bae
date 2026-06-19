import AppKit
import os.log

private let logger = Logger.bae("UiEventReducer")

/// Reduces BridgeUiEvents into the appropriate stores. High-frequency
/// events go directly to NSViews via Combine subjects on AppService.
/// Everything else lands on the store that owns the field.
enum UiEventReducer {
    @MainActor
    static func reduce(
        _ event: BridgeUiEvent,
        playbackStore: PlaybackStore,
        configStore: ConfigStore,
        importStore: ImportStore,
        libraryStore: LibraryStore,
        appService: AppService,
        uiStore: UiStore,
        outboxStore: OutboxStore,
        downloadStore: DownloadStore
    ) {
        switch event {
        // ── Playback ───────────────────────────────────────────────────
        case .playbackPlaying(
            let trackId,
            let trackTitle,
            let artistNames,
            let artistId,
            let albumId,
            let albumTitle,
            let coverImageId,
            let durationMs,
            let durationLabel
        ):
            playbackStore.nowPlaying = .playing(
                NowPlayingTrack(
                    trackId: trackId,
                    trackTitle: trackTitle,
                    artistNames: artistNames,
                    albumId: albumId,
                    coverImageId: coverImageId,
                    durationMs: durationMs,
                    durationLabel: durationLabel,
                )
            )
            let state = BridgePlaybackState.playing(
                trackId: trackId,
                trackTitle: trackTitle,
                artistNames: artistNames,
                artistId: artistId,
                albumId: albumId,
                albumTitle: albumTitle,
                coverImageId: coverImageId,
                durationMs: durationMs,
                durationLabel: durationLabel,
            )
            appService.mediaControlService.updateNowPlaying(
                state: state,
                appHandle: appService.appHandle
            )

        case .playbackPaused(
            let trackId,
            let trackTitle,
            let artistNames,
            let artistId,
            let albumId,
            let albumTitle,
            let coverImageId,
            let durationMs,
            let durationLabel
        ):
            playbackStore.nowPlaying = .paused(
                NowPlayingTrack(
                    trackId: trackId,
                    trackTitle: trackTitle,
                    artistNames: artistNames,
                    albumId: albumId,
                    coverImageId: coverImageId,
                    durationMs: durationMs,
                    durationLabel: durationLabel,
                )
            )
            let state = BridgePlaybackState.paused(
                trackId: trackId,
                trackTitle: trackTitle,
                artistNames: artistNames,
                artistId: artistId,
                albumId: albumId,
                albumTitle: albumTitle,
                coverImageId: coverImageId,
                durationMs: durationMs,
                durationLabel: durationLabel,
            )
            appService.mediaControlService.updateNowPlaying(
                state: state,
                appHandle: appService.appHandle
            )

        case .playbackLoading(let trackId, let track):
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
                        durationMs: track.durationMs,
                        durationLabel: track.durationLabel
                    )
                )
            }
            else {
                playbackStore.beginLoading(trackId: trackId)
                playbackStore.playbackPositionSubject.send(.reset)
            }
            appService.mediaControlService.updateNowPlaying(
                state: .loading(trackId: trackId, track: track),
                appHandle: appService.appHandle
            )

        case .playbackStopped:
            playbackStore.nowPlaying = .stopped
            playbackStore.playbackPositionSubject.send(.reset)
            appService.mediaControlService.updateNowPlaying(
                state: .stopped,
                appHandle: appService.appHandle
            )

        case .playbackError(let message):
            // A track couldn't be played (cloud-only not downloaded, decode
            // failure); core has already fallen back to stopped. Surface why.
            uiStore.showError(message)

        case .playbackProgress(
            let positionMs,
            let durationMs,
            let progress,
            let elapsedLabel,
            let remainingLabel
        ):
            playbackStore.playbackPositionSubject.send(
                .position(
                    progress: progress,
                    elapsed: elapsedLabel,
                    remaining: remainingLabel
                )
            )
            appService.mediaControlService.updatePosition(
                positionMs: positionMs,
                durationMs: durationMs
            )

        case .volumeChanged(let volume):
            playbackStore.volume = volume

        case .muteChanged(let isMuted):
            playbackStore.isMuted = isMuted

        case .repeatModeChanged(let mode):
            playbackStore.repeatMode = RepeatMode(bridge: mode)

        case .queueUpdated(let items, let hasNext, let hasPrevious):
            // The event carries display-ready items (core resolves them before
            // emitting), so map them straight through — no DB re-query here.
            playbackStore.queueItems = items.map(QueueItem.init(bridge:))
            appService.mediaControlService.updateCommandAvailability(
                hasNext: hasNext,
                hasPrevious: hasPrevious
            )

        case .queueItemsAdded(let count):
            playbackStore.queueItemsAddedSubject.send(Int(count))

        // ── Preview ────────────────────────────────────────────────────
        case .previewPlaying(let path, let durationMs, let durationLabel):
            importStore.previewState = .playing(
                path: path,
                durationMs: durationMs,
                durationLabel: durationLabel
            )
            appService.mediaControlService.updateNowPlayingForPreview(
                state: .playing(
                    path: path,
                    durationMs: durationMs,
                    durationLabel: durationLabel
                )
            )

        case .previewPaused(let path, let durationMs, let durationLabel):
            importStore.previewState = .paused(
                path: path,
                durationMs: durationMs,
                durationLabel: durationLabel
            )
            appService.mediaControlService.updateNowPlayingForPreview(
                state: .paused(
                    path: path,
                    durationMs: durationMs,
                    durationLabel: durationLabel
                )
            )

        case .previewIdle:
            importStore.previewState = .idle
            importStore.previewProgressSubject.send(.reset)
            appService.mediaControlService.updateNowPlayingForPreview(
                state: .idle
            )

        case .previewProgress(let positionMs, let progress, let elapsedLabel):
            importStore.previewProgressSubject.send(
                .position(progress: progress, elapsed: elapsedLabel)
            )
            appService.mediaControlService.updatePreviewPosition(
                positionMs: positionMs
            )

        // ── Candidate-scoped ───────────────────────────────────────────
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

        case .candidateImportComplete(let key, let releaseId, let albumId):
            importStore.mutateCandidate(forKey: key) {
                $0.importStatus = .complete(
                    albumId: albumId,
                    releaseId: releaseId
                )
            }

        case .candidateImportError(let key, let message):
            importStore.mutateCandidate(forKey: key) {
                $0.importStatus = .error(message: message)
            }

        // ── Scan ───────────────────────────────────────────────────────
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

        case .candidateSkipChanged(let key, let skipped):
            // The user skipped or unskipped the candidate; flip its flag so the
            // import view re-tabs it New ↔ Skipped. In-place mutation keeps the
            // candidate's identify/search/import state.
            importStore.mutateCandidate(forKey: key) { $0.skipped = skipped }

        case .scanFinished:
            // No state change needed — views react to candidate additions
            break

        // ── Library ────────────────────────────────────────────────────
        case .albumAdded(let album):
            logger.info(
                "reducer: albumAdded for album \(album.album.id, privacy: .public)"
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
            importStore.handleAlbumRemoved(
                albumId: albumId,
                releaseIds: releaseIds
            )
            uiStore.clearSelectedRelease(inAlbum: albumId)
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
            importStore.handleReleaseRemoved(releaseId: releaseId)
            uiStore.clearSelectedReleaseIfMatching(releaseId, inAlbum: albumId)
            libraryStore.libraryShapeSubject.send(
                .releaseRemoved(albumId: albumId, releaseId: releaseId)
            )

        case .configChanged(let config, let syncReady):
            configStore.config = Config(bridge: config)
            configStore.syncReady = syncReady

        case .syncError(let message):
            configStore.syncError = message

        case .syncTimeChanged(let time):
            // `nil` time means no sync has completed yet — a real absence, so
            // the Date stays nil. Otherwise core sends epoch milliseconds.
            configStore.lastSyncTime = time.map {
                Date(timeIntervalSince1970: TimeInterval($0) / 1000)
            }

        case .syncingChanged(let syncing):
            configStore.syncing = syncing

        case .outboxChanged(let snapshot):
            outboxStore.snapshot = snapshot

        case .downloadQueueChanged(let snapshot):
            downloadStore.snapshot = snapshot

        case .releaseTransferProgress(
            let releaseId,
            let action,
            let fileNo,
            let total,
            let percent
        ):
            libraryStore.handleReleaseTransferProgress(
                releaseId: releaseId,
                percent: percent,
                label: transferProgressLabel(
                    action: action,
                    fileNo: fileNo,
                    total: total,
                    percent: percent
                )
            )

        case .releaseTransferEnded(let releaseId):
            libraryStore.handleReleaseTransferEnded(releaseId: releaseId)

        // ── Errors ─────────────────────────────────────────────────────
        case .error(let message):
            uiStore.showError(message)

        case .errorCleared:
            uiStore.clearError()
        }
    }
}
