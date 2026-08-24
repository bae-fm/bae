import BaeKit

@MainActor
final class DesktopEventHandler {
    private let importStore: ImportStore
    private let mediaControlService: MediaControlService
    /// Where a background failure becomes something the user sees: the global
    /// alert every other caught error is routed to.
    private let uiStore: UiStore

    init(
        importStore: ImportStore,
        mediaControlService: MediaControlService,
        uiStore: UiStore
    ) {
        self.importStore = importStore
        self.mediaControlService = mediaControlService
        self.uiStore = uiStore
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
                    fraction: fraction.map(Double.init)
                )
            )

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

        // A folder the user is watching could not be read. Its entry in the
        // import list's menu keeps the lasting mark; this is the moment it
        // broke, and core sends it once per distinct failure rather than on
        // every re-scan, so the alert is news every time it appears.
        case .watchedFolderScanFailed(let watchedFolderPath, let detail):
            uiStore.showError(
                DisplayError(
                    line: String(
                        format: NSLocalizedString(
                            "ui.import.folder.scan_failed",
                            tableName: "Core",
                            bundle: .main,
                            comment: ""
                        ),
                        watchedFolderPath
                    ),
                    detail: detail
                )
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
