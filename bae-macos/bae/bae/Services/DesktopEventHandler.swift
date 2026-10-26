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

        case .importIdentificationProgress(let identified, let total):
            importStore.identificationProgress = (
                identified: identified, total: total
            )

        case .importsInFlight(let count):
            importStore.importsInFlight = count

        case .playbackError, .queueItemsAdded:
            preconditionFailure("Unhandled UI event \(event)")
        }
    }

    func applyPreview(_ state: BridgePreviewState) {
        importStore.previewState = state
    }

    /// Show where the preview is in its clip, or clear the bar while no
    /// preview is loaded.
    func applyPreviewPosition(_ position: BridgePreviewPosition?) {
        guard let position else {
            importStore.previewProgressSubject.send(.reset)
            return
        }
        importStore.previewProgressSubject.send(
            .position(
                progress: position.progress,
                positionMs: Int64(position.positionMs),
                durationMs: position.durationMs
            )
        )
    }
}
