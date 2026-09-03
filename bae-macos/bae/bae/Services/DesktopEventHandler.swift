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
        switch values.state {
        case .playing(let target, let durationMs):
            importStore.previewState = .playing(
                target: target,
                durationMs: durationMs
            )
        case .paused(let target, let durationMs):
            importStore.previewState = .paused(
                target: target,
                durationMs: durationMs
            )
        case .idle:
            importStore.previewState = .idle
        }
        importStore.previewProgressSubject.send(
            values.state == .idle
                ? .reset
                : .position(
                    progress: values.progress,
                    positionMs: values.positionMs
                )
        )
    }
}
