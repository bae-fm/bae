import BaeKit

/// The events the shared `UiEventDispatcher` declines that macOS still handles
/// itself: import preview playback and per-track loudness-measurement progress
/// during an import. Anything else the dispatcher declines is dropped — macOS's
/// policy for events no shared arm consumes.
enum DesktopUiEvents {
    @MainActor
    static func apply(_ event: BridgeUiEvent, appService: AppService) {
        switch event {
        case .previewPlaying(let path, let durationMs):
            appService.importStore.previewState = .playing(
                path: path,
                durationMs: durationMs
            )
            appService.mediaControlService.updateNowPlayingForPreview(
                state: .playing(path: path, durationMs: durationMs)
            )

        case .previewPaused(let path, let durationMs):
            appService.importStore.previewState = .paused(
                path: path,
                durationMs: durationMs
            )
            appService.mediaControlService.updateNowPlayingForPreview(
                state: .paused(path: path, durationMs: durationMs)
            )

        case .previewIdle:
            appService.importStore.previewState = .idle
            appService.importStore.previewProgressSubject.send(.reset)
            appService.mediaControlService.updateNowPlayingForPreview(
                state: .idle
            )

        case .previewProgress(let positionMs, let progress):
            appService.importStore.previewProgressSubject.send(
                .position(
                    progress: progress,
                    elapsed: DurationClock.text(Int64(positionMs))
                )
            )
            appService.mediaControlService.updatePreviewPosition(
                positionMs: positionMs
            )

        case .candidateImportLoudnessProgress(
            let key,
            let tracksDone,
            let tracksTotal,
            let fraction
        ):
            appService.importStore.importLoudnessSubject.send(
                ImportLoudnessProgressEvent(
                    key: key,
                    tracksDone: tracksDone,
                    tracksTotal: tracksTotal,
                    fraction: Double(fraction)
                )
            )

        default:
            break
        }
    }
}
