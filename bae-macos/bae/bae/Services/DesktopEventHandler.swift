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
        case .candidateSignalsUpdated(let key, let signals):
            importStore.candidateSignalsSubject.send(
                CandidateSignalsEvent(
                    key: key,
                    signals: Signals(bridge: signals)
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
        case .playing(let target, let durationMs):
            importStore.previewState = .playing(
                target: target,
                durationMs: durationMs
            )
            mediaControlService.updateNowPlayingForPreview(state: values.state)
        case .paused(let target, let durationMs):
            importStore.previewState = .paused(
                target: target,
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
