import BaeKit

/// The events the shared `UiEventDispatcher` declines that macOS still handles
/// itself: import preview playback, per-track loudness-measurement progress
/// during an import, the import queue's identify progress, and cast-status
/// changes. Anything else the dispatcher declines is dropped — macOS's policy
/// for events no shared arm consumes.
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
                .position(progress: progress, positionMs: positionMs)
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

        case .importQueueIdentifyProgress(let identified, let total):
            appService.importStore.queueIdentifyProgress = (
                identified: identified, total: total
            )
            // The sweep answers hundreds of candidates in the background
            // without invalidating each one individually — see
            // `wire_import_events`'s comment on why — so this tick is the
            // signal that the row list may have moved, not just the header
            // line. `Projection` cancels a still-running reload the moment
            // the next one arrives, so a burst of ticks collapses into the
            // one query that finishes after the burst settles rather than
            // running the full triage read once per candidate.
            appService.projectionRegistry.invalidate(.importCandidateList)

        default:
            break
        }
    }
}
