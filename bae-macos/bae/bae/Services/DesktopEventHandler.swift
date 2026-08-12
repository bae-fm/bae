import BaeKit

@MainActor
final class DesktopEventHandler {
    private let importStore: ImportStore
    private let mediaControlService: MediaControlService

    init(
        importStore: ImportStore,
        mediaControlService: MediaControlService
    ) {
        self.importStore = importStore
        self.mediaControlService = mediaControlService
    }

    func apply(_ event: BridgeUiEvent) {
        switch event {
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

        case .playbackError, .queueItemsAdded, .error:
            preconditionFailure("Unhandled UI event \(event)")
        }
    }

    func apply(_ values: BridgePreviewValues) {
        switch values.state {
        case .playing(let path, let durationMs):
            importStore.previewState = .playing(
                path: path,
                durationMs: durationMs
            )
            mediaControlService.updateNowPlayingForPreview(state: values.state)
        case .paused(let path, let durationMs):
            importStore.previewState = .paused(
                path: path,
                durationMs: durationMs
            )
            mediaControlService.updateNowPlayingForPreview(state: values.state)
        case .idle:
            importStore.previewState = .idle
            mediaControlService.updateNowPlayingForPreview(state: .idle)
        }
        importStore.previewProgressSubject.send(
            values.state == .idle
                ? .reset
                : .position(
                    progress: values.progress,
                    positionMs: values.positionMs
                )
        )
        mediaControlService.updatePreviewPosition(positionMs: values.positionMs)
    }
}
