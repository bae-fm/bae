import AppKit
import BaeKit
import OSLog
import SwiftUI

// MARK: - AlbumDetailView (wiring view)

struct AlbumDetailView: View {
    let albumId: String
    @Environment(Playback.self)
    var playback
    @Environment(Queue.self)
    var queue
    @Environment(Library.self)
    var library
    @Environment(ReleaseEditor.self)
    var releaseEditor
    @Environment(Importer.self)
    var importer
    @Environment(Sync.self)
    var sync
    @Environment(Downloads.self)
    var downloads
    @Environment(Outputs.self)
    var outputs
    @Environment(ConfigStore.self)
    var configStore
    @Environment(TrackSave.self)
    var export
    @Environment(PlaybackStore.self)
    var playbackStore
    @Environment(UiStore.self)
    var uiStore
    // Absent in a window that hosts the detail without the library's reads.
    @Environment(LibraryProjectionStore.self)
    var libraryProjections: LibraryProjectionStore?
    @Environment(SettingsNavigation.self)
    var settingsNavigation
    @Environment(\.openSettingsWindow)
    var openSettings

    @State
    private var transferError: DisplayError?
    @State
    private var storageTask: Task<Void, Never>?
    /// The release the menu asked to delete, while its confirmation shows.
    @State
    private var releaseIdPendingDelete: String?
    /// The album the menu asked to delete, while its confirmation shows.
    @State
    private var albumIdPendingDelete: String?
    @State
    private var deleteError: DisplayError?
    @State
    private var exportError: DisplayError?
    @State
    private var exportTask: Task<Void, Never>?
    /// The view keeps one album detail reader and asks it to show `albumId`
    /// and the release chosen on its page whenever either changes.
    @State
    private var detailReader:
        KeyedDetailReader<BridgeAlbumDetail, BridgeAlbumDetailRequest>?
    /// What the read last returned for the album shown, or why it failed.
    @State
    private var albumRead = LibraryProjectionState<AlbumDetail>()

    var body: some View {
        Group {
            if let detail = albumRead.value {
                let selectedDetail = detail.shownRelease
                let selectedReleaseId = selectedDetail.id
                MacAlbumPanel(
                    detail: detail,
                    onBrowseImages: {
                        presentImages(release: selectedDetail)
                    },
                    onSelectRelease: { releaseId in
                        selectRelease(releaseId, in: detail)
                    },
                    currentTrackId: playbackStore.nowPlaying.trackId,
                    currentReleaseId: playbackStore.nowPlaying.display?.library?
                        .releaseId,
                    isPlaying: playbackStore.nowPlaying.isPlaying,
                    onClose: { uiStore.closeAlbumDetail() },
                    onPlay: {
                        playback.playRelease(selectedReleaseId, nil, false)
                    },
                    onShuffle: {
                        playback.playRelease(selectedReleaseId, nil, true)
                    },
                    onPlayFromTrack: { trackId in
                        playback.playRelease(
                            selectedReleaseId,
                            trackId,
                            false
                        )
                    },
                    onTogglePlayPause: {
                        playback.playPause(for: playbackStore.nowPlaying)
                    },
                    onAddNext: { trackId in
                        queue.addNext([trackId])
                    },
                    onAddToQueue: { trackId in
                        queue.addToQueue([trackId])
                    },
                    onAddNextAlbum: {
                        queue.addNext(selectedDetail.tracks.map(\.id))
                    },
                    onAddAlbumToQueue: {
                        queue.addToQueue(selectedDetail.tracks.map(\.id))
                    },
                    onEdit: {
                        presentReleaseEditor(release: selectedDetail)
                    },
                    onOpenStorage: {
                        presentReleaseStorageSheet(
                            selectedDetail,
                            albumTitle: detail.album.title
                        )
                    },
                    onExportRelease: {
                        exportRelease(releaseId: selectedReleaseId)
                    },
                    onSaveReleaseAs: {
                        saveReleaseAs(releaseId: selectedReleaseId)
                    },
                    onSetPrimaryRelease: {
                        setPrimaryRelease(releaseId: selectedReleaseId)
                    },
                    onDeleteRelease: {
                        releaseIdPendingDelete = selectedReleaseId
                    },
                    onDeleteAlbum: {
                        albumIdPendingDelete = albumId
                    },
                    onExportTrack: { trackId in
                        exportTrack(trackId: trackId)
                    },
                    onOpenWork: { workId in
                        uiStore.navigateToWork(workId)
                    },
                    onRateAlbum: { stars in
                        rate(.album(id: albumId), stars)
                    },
                    onRateTrack: { trackId, stars in
                        rate(.track(id: trackId), stars)
                    },
                    playlists: trackPlaylistChoices,
                    picking: trackPicking
                )
            }
            else {
                detailPlaceholder
            }
        }
        .onAppear { showAlbum() }
        .onChange(of: albumRequest) { showAlbum() }
        // The picks are tracks of the release the page shows, so the page
        // clears them when it shows another release or closes.
        .onChange(of: albumRead.value?.shownRelease.id) {
            uiStore.pickedTracks.clear()
        }
        .onDisappear {
            uiStore.pickedTracks.clear()
            detailReader?.close()
            uiStore.dismissAllModals()
            exportTask?.cancel()
            storageTask?.cancel()
        }
        .errorAlert("Transfer failed", error: $transferError)
        .errorAlert("Export failed", error: $exportError)
        .errorAlert("Delete failed", error: $deleteError)
        .macAlert(item: $releaseIdPendingDelete) { releaseId in
            DeleteReleaseConfirmation.alert {
                let releaseEditor = releaseEditor
                // Close the detail only once the delete succeeds: a failure keeps
                // the release in view and surfaces the error, rather than closing
                // over a release that is still there.
                Task {
                    do {
                        try await releaseEditor.deleteRelease(releaseId)
                        uiStore.closeAlbumDetail()
                    }
                    catch {
                        deleteError = DisplayError(error)
                    }
                }
            }
        }
        .deleteAlbumConfirmation(albumId: $albumIdPendingDelete) { albumId in
            let releaseEditor = releaseEditor
            // As for a release: the detail closes only once the delete
            // succeeds, and a failure keeps the album in view with the error.
            Task {
                do {
                    try await releaseEditor.deleteAlbum(albumId)
                    uiStore.closeAlbumDetail()
                }
                catch {
                    deleteError = DisplayError(error)
                }
            }
        }
    }

    // MARK: - Data helpers

    /// What the view reads: the album, and the release chosen on its page.
    private var albumRequest: BridgeAlbumDetailRequest {
        BridgeAlbumDetailRequest(
            albumId: albumId,
            chosenReleaseId: uiStore.selectedReleaseIdByAlbum[albumId]
        )
    }

    /// Point the view's one read at `albumRequest`. When the request names
    /// another album, `showAlbum` clears what the read returned for the last
    /// album. When the request names another release of the same album, the
    /// view keeps drawing the last value until the new release's value
    /// arrives.
    private func showAlbum() {
        let request = albumRequest
        if detailReader?.id?.albumId != request.albumId {
            albumRead = LibraryProjectionState()
        }
        let reader =
            detailReader
            ?? KeyedDetailReader(
                open: library.albumDetail,
                onValue: { _, value in
                    albumRead = LibraryProjectionState(
                        value: value.map(AlbumDetail.init(bridge:)),
                        delivered: true
                    )
                },
                onError: { _, error in
                    albumRead = LibraryProjectionState(
                        value: albumRead.value,
                        delivered: albumRead.delivered,
                        error: DisplayError(error)
                    )
                }
            )
        detailReader = reader
        reader.show(request)
    }

    /// What shows before the album detail subscription delivers its first
    /// value: nothing, since a local read lands at once, or why it failed,
    /// with Retry.
    @ViewBuilder
    private var detailPlaceholder: some View {
        if let error = albumRead.error {
            MacNotice(
                error: error,
                look: .centered,
                action: error.retryAction {
                    detailReader?.retry()
                }
            )
        }
        else {
            Color.clear
        }
    }

    /// Show the release `releaseId`, which the person picked. Picking the
    /// primary release clears the choice instead of storing it.
    private func selectRelease(_ releaseId: String, in detail: AlbumDetail) {
        if releaseId == detail.album.primaryReleaseId {
            uiStore.clearSelectedRelease(inAlbum: albumId)
        }
        else {
            uiStore.selectRelease(releaseId, inAlbum: albumId)
        }
    }
}

// MARK: - Modal presenters

extension AlbumDetailView {
    /// The release's images, larger, with the cover picker's choices: a
    /// pick there writes the cover at once.
    private func presentImages(release: ReleaseDetail) {
        let selectedReleaseId = release.id
        let releaseEditor = releaseEditor
        // One for as long as the sheet is up: the modal's view is built
        // again with each update of the window.
        let lightboxLoad = LightboxImageLoad()
        uiStore.presentModal {
            CoverPickerFrame {
                CoverSheetView(
                    owner: .release(releaseId: selectedReleaseId),
                    lightboxLoad: lightboxLoad,
                    onFindRelease: {
                        uiStore.dismissModal()
                        presentReleaseEditor(release: release)
                    },
                    refreshCovers: {
                        try await releaseEditor.refreshReleaseCovers(
                            selectedReleaseId
                        )
                    },
                    onSelect: { selection in
                        try await releaseEditor.changeCover(
                            selectedReleaseId,
                            selection
                        )
                    },
                    onDone: { uiStore.dismissModal() }
                )
            }
        }
    }

    /// Open the one release editor on `release`, as the library holds it.
    private func presentReleaseEditor(release: ReleaseDetail) {
        let releaseId = release.id
        let displayName = release.displayName
        let currentAlbumId = albumId
        let releaseEditor = releaseEditor
        let importer = importer
        Task {
            do {
                let opened = try await releaseEditor.openReleaseEdit(releaseId)
                uiStore.presentModal {
                    ReleaseEditorSheet(
                        releaseId: releaseId,
                        displayName: displayName,
                        opened: opened,
                        commands: releaseEditor.editCommands(
                            searchAlbums: importer.searchAlbums
                        ),
                        onSaved: { albumId in
                            uiStore.dismissAllModals()
                            // A release the save moved is shown where it went.
                            if albumId != currentAlbumId {
                                uiStore.navigateToAlbum(
                                    albumId,
                                    releaseId: releaseId
                                )
                            }
                        },
                        onClose: { uiStore.dismissAllModals() }
                    )
                }
            }
            catch {
                uiStore.showError(error)
            }
        }
    }

    private func presentReleaseStorageSheet(
        _ detail: ReleaseDetail,
        albumTitle: String
    ) {
        uiStore.presentModal {
            ReleaseStorageSheet(
                release: detail,
                albumTitle: albumTitle,
                asksBeforeUnpinning: configStore.config.askBeforeUnpinning,
                onAction: { action in
                    performStorageAction(action, releaseId: detail.id)
                },
                onUnpinConfirmed: { dontShowAgain in
                    unpinRelease(
                        releaseId: detail.id,
                        steps: UnpinAsk.steps(
                            after: .unpin(dontShowAgain: dontShowAgain)
                        )
                    )
                },
                onCancelPin: { cancelPin(releaseId: detail.id) },
                onExport: { exportRelease(releaseId: detail.id) },
                onSaveAs: { saveReleaseAs(releaseId: detail.id) },
                onDone: { uiStore.dismissModal() },
            )
            .frame(width: 640, height: 600)
            .background(Theme.surfaceElevated)
        }
    }

    private func performStorageAction(
        _ action: BridgeReleaseStorageAction,
        releaseId: String
    ) {
        switch action {
        case .pin:
            pinRelease(releaseId: releaseId)
        case .unpin:
            unpinRelease(releaseId: releaseId, steps: UnpinAsk.withoutAsking)
        case .upload:
            presentMoveToCloudConfirmSheet(releaseId: releaseId)
        case .moveToThisDevice:
            makeReleaseLocal(releaseId: releaseId)
        }
    }

    /// Export a release verbatim to a folder — a pure copy-out that reproduces
    /// the imported file set. Changes no state, so it's offered regardless of the
    /// release's locality. The copy runs on the output queue and surfaces in the
    /// Storage Manager's inspector for the release.
    private func exportRelease(releaseId: String) {
        guard let targetDir = OutputTarget.resolveExportDir() else {
            return
        }
        Task {
            do {
                try await outputs.enqueueExport(releaseId, targetDir)
            }
            catch {
                exportError = DisplayError(error)
            }
        }
    }

    /// Save a release under a chosen preset + folder — a rendered workup (decode,
    /// encode, tags, cover) rather than a verbatim copy. Runs on the same output
    /// queue as export.
    private func saveReleaseAs(releaseId: String) {
        Task {
            guard
                let target = await OutputTarget.resolveReleaseSave(
                    configStore: configStore,
                    outputs: outputs,
                    openFormats: openSaveAsSettings
                )
            else {
                return
            }
            do {
                try await outputs.enqueueReleaseSave(
                    releaseId,
                    target.targetDir,
                    target.presetId
                )
            }
            catch {
                exportError = DisplayError(error)
            }
        }
    }

    /// Settings at the Save as pane, in front of whichever save panel asked.
    private func openSaveAsSettings() {
        settingsNavigation.open(.formats, present: { openSettings() })
    }

    private func presentMoveToCloudConfirmSheet(releaseId: String) {
        uiStore.presentModal {
            MoveToCloudConfirmSheet(
                onConfirm: { moveReleaseToCloud(releaseId: releaseId) },
                onCancel: { uiStore.dismissModal() },
            )
            .frame(width: 420)
        }
    }

    // MARK: - Actions

    /// Run one storage transition with the shared cancel-prior/error plumbing.
    /// `transition` is the single bridge call that
    /// differs per action. The bridge call is async (it descends into the cloud
    /// future chain on a runtime worker); progress is rendered from the
    /// subscribed release summary, not from local view state.
    private func runStorageTransition(
        _ transition: @escaping @Sendable () async throws -> Void
    ) {
        transferError = nil
        storageTask?.cancel()
        storageTask = Task {
            do {
                try await transition()
            }
            catch is CancellationError {}
            catch {
                transferError = DisplayError(error)
            }
        }
    }

    private func pinRelease(releaseId: String) {
        // Pinning enqueues on the download queue rather than awaiting
        // a per-release transition. The sheet's download status tracks the
        // entry from subscribed release values; the storage state flips when
        // the live album detail delivers the worker's database commit.
        // Queueing itself can fail, and shows why where the other transitions do.
        let downloads = downloads
        runStorageTransition {
            try await downloads.queuePins([releaseId])
        }
    }

    /// End a pin that waits in the download queue.
    private func cancelPin(releaseId: String) {
        let downloads = downloads
        runStorageTransition {
            try await downloads.cancelDownload(releaseId)
        }
    }

    private func unpinRelease(releaseId: String, steps: [UnpinAsk.Step]) {
        let downloads = downloads
        runStorageTransition {
            try await UnpinAsk.run(
                steps,
                stopAsking: {
                    try await downloads.setAskBeforeUnpinning(false)
                },
                unpin: { try await downloads.unpinReleases([releaseId]) }
            )
        }
    }

    private func moveReleaseToCloud(releaseId: String) {
        uiStore.dismissModal()
        let releaseEditor = releaseEditor
        runStorageTransition {
            try await releaseEditor.moveReleaseToCloud(releaseId)
        }
    }

    private func makeReleaseLocal(releaseId: String) {
        guard
            let newPath =
                StorageActionRunner.promptMoveToThisDeviceDestination()
        else {
            return
        }
        let releaseEditor = releaseEditor
        runStorageTransition {
            try await releaseEditor.moveReleasesToThisDevice(
                [releaseId],
                newPath
            )
        }
    }

    private func setPrimaryRelease(releaseId: String) {
        let releaseEditor = releaseEditor
        let albumId = albumId
        Task {
            do {
                try await releaseEditor.setPrimaryRelease(albumId, releaseId)
            }
            catch {
                // No line means a cancellation, which raises no alert. The
                // typed failure carries through so the alert can still show the
                // fault and offer Copy details.
                guard let displayed = DisplayError(error) else { return }
                uiStore.showError(
                    displayed.addingContext(
                        String(localized: "Failed to set primary release")
                    )
                )
            }
        }
    }

    private func exportTrack(trackId: String) {
        exportTask?.cancel()
        exportTask = Task {
            // Core renders the suggested stem from the library's one filename
            // pattern; a failure surfaces rather than falling back to a raw
            // title.
            let stem: String
            do {
                stem = try await export.suggestedName(trackId)
            }
            catch is CancellationError {
                return
            }
            catch {
                exportError = DisplayError(error)
                return
            }

            let panel = TrackSavePanel.make(
                suggestedStem: stem,
                configStore: configStore,
                outputs: outputs,
                openFormats: openSaveAsSettings
            )
            defer { panel.picker.stop() }
            let response = await withCheckedContinuation { continuation in
                panel.savePanel.begin { continuation.resume(returning: $0) }
            }
            guard response == .OK else {
                return  // user cancelled the save panel
            }
            guard let url = panel.savePanel.url else {
                Logger.bae("export")
                    .warning(
                        "save panel returned .OK with no URL; skipping save"
                    )
                return
            }

            let presetId = panel.picker.selectedPresetId
            do {
                try await export.saveTrack(trackId, url, presetId)
            }
            catch is CancellationError {
                // view dismissed mid-save; OutputFileGuard cleaned up
            }
            catch {
                exportError = DisplayError(error)
            }
        }
    }
}
