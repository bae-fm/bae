import BaeKit

/// The events the shared `UiEventDispatcher` declines: import preview playback
/// and per-track loudness-measurement progress during an import — neither of
/// which iOS has a surface for (no import preview player). Exhaustive, no
/// `default`: a `BridgeUiEvent` variant the dispatcher declines to handle in
/// the future forces an explicit decision here. Every event a shared arm
/// should already have consumed reaching this tail is a bug, so those cases
/// trap instead of silently dropping.
enum DesktopUiEvents {
    @MainActor
    static func ignore(_ event: BridgeUiEvent, _: AppService) {
        switch event {
        case .previewIdle, .previewPlaying, .previewPaused, .previewProgress,
            .candidateImportLoudnessProgress:
            break

        case .invalidated, .playbackStopped, .playbackError, .playbackLoading,
            .playbackPlaying, .playbackPaused, .playbackProgress,
            .playbackSeeked, .volumeChanged, .muteChanged, .repeatModeChanged,
            .queueUpdated, .queueItemsAdded, .releaseTransferProgress,
            .releaseTransferEnded, .error, .errorCleared:
            preconditionFailure("Unhandled UI event \(event)")
        }
    }
}
