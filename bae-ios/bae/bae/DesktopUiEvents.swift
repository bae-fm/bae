import BaeKit

/// The events the shared `UiEventDispatcher` declines: import preview playback
/// and the per-track loudness-measurement and identify progress of an import —
/// none of which iOS has a surface for (no import flow). Exhaustive, no
/// `default`: a `BridgeUiEvent` variant the dispatcher declines to handle in
/// the future forces an explicit decision here. Every event a shared arm
/// should already have consumed reaching this tail is a bug, so those cases
/// trap instead of silently dropping.
enum DesktopUiEvents {
    @MainActor
    static func ignore(_ event: BridgeUiEvent) {
        switch event {
        case .candidateImportLoudnessProgress, .importQueueIdentifyProgress:
            break

        case .playbackError, .queueItemsAdded, .error:
            preconditionFailure("Unhandled UI event \(event)")
        }
    }
}
