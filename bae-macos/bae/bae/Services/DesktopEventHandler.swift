import BaeKit

@MainActor
final class DesktopEventHandler {
    private let importStore: ImportStore

    init(importStore: ImportStore) {
        self.importStore = importStore
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
        importStore.previewState = values.state
        let position: PlaybackPositionEvent
        switch values.state {
        case .playing(_, let durationMs), .paused(_, let durationMs):
            position = .position(
                progress: values.progress,
                positionMs: Int64(values.positionMs),
                durationMs: durationMs
            )
        case .idle:
            position = .reset
        }
        importStore.previewProgressSubject.send(position)
    }
}
