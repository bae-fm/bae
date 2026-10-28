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

    /// Playback values arrive as often as the volume slider moves; the
    /// preview's state is written, and its readers redrawn, only when it
    /// changes.
    func applyPreview(_ state: BridgePreviewState) {
        guard importStore.previewState != state else { return }
        importStore.previewState = state
    }
}
