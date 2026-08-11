import BaeKit

@MainActor
final class DesktopEventHandler {
    private let importStore: ImportStore
    private let projectionRegistry: ProjectionRegistry
    private let mediaControlService: MediaControlService

    init(
        importStore: ImportStore,
        projectionRegistry: ProjectionRegistry,
        mediaControlService: MediaControlService
    ) {
        self.importStore = importStore
        self.projectionRegistry = projectionRegistry
        self.mediaControlService = mediaControlService
    }

    func apply(_ event: BridgeUiEvent) {
        switch event {
        case .previewPlaying(let path, let durationMs):
            importStore.previewState = .playing(
                path: path,
                durationMs: durationMs
            )
            mediaControlService.updateNowPlayingForPreview(
                state: .playing(path: path, durationMs: durationMs)
            )

        case .previewPaused(let path, let durationMs):
            importStore.previewState = .paused(
                path: path,
                durationMs: durationMs
            )
            mediaControlService.updateNowPlayingForPreview(
                state: .paused(path: path, durationMs: durationMs)
            )

        case .previewIdle:
            importStore.previewState = .idle
            importStore.previewProgressSubject.send(.reset)
            mediaControlService.updateNowPlayingForPreview(state: .idle)

        case .previewProgress(let positionMs, let progress):
            importStore.previewProgressSubject.send(
                .position(progress: progress, positionMs: positionMs)
            )
            mediaControlService.updatePreviewPosition(positionMs: positionMs)

        case .candidateImportLoudnessProgress(
            let key,
            let tracksDone,
            let tracksTotal,
            let fraction
        ):
            importStore.importLoudnessSubject.send(
                ImportLoudnessProgressEvent(
                    key: key,
                    tracksDone: tracksDone,
                    tracksTotal: tracksTotal,
                    fraction: Double(fraction)
                )
            )

        case .importQueueIdentifyProgress(let identified, let total):
            importStore.queueIdentifyProgress = (
                identified: identified, total: total
            )
            projectionRegistry.invalidate(.importCandidateList)

        case .castStatusChanged, .invalidated, .playbackStopped, .playbackError,
            .playbackLoading, .playbackPlaying, .playbackPaused,
            .playbackProgress, .playbackSeeked, .volumeChanged, .muteChanged,
            .repeatModeChanged, .queueUpdated, .queueItemsAdded,
            .releaseTransferProgress, .releaseTransferEnded, .error:
            preconditionFailure("Unhandled UI event \(event)")
        }
    }
}
