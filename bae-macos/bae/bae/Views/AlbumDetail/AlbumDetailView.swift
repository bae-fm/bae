import AppKit
import BaeKit
import OSLog
import SwiftUI

// MARK: - AlbumDetailView (wiring view)

struct AlbumDetailView: View {
    let albumId: String
    @Environment(MediaPaths.self)
    var mediaPaths
    @Environment(Playback.self)
    var playback
    @Environment(Queue.self)
    var queue
    @Environment(Library.self)
    var library
    @Environment(ReleaseEditor.self)
    var releaseEditor
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
    @Environment(LibraryStore.self)
    var libraryStore
    @Environment(PlaybackStore.self)
    var playbackStore
    @Environment(UiStore.self)
    var uiStore

    @State
    private var coverChangeError: String?
    @State
    private var changeCoverTask: Task<Void, Never>?
    @State
    private var coverSheetTask: Task<Void, Never>?
    @State
    private var transferError: String?
    @State
    private var storageTask: Task<Void, Never>?
    @State
    private var showingDeleteConfirmation: Bool = false
    @State
    private var releaseIdPendingDelete: String?
    @State
    private var deleteError: String?
    @State
    private var exportError: String?
    @State
    private var exportTask: Task<Void, Never>?

    var body: some View {
        Group {
            if let summary = libraryStore.albumSummaries[albumId] {
                let selectedReleaseId = activeReleaseId(summary: summary)
                let selectedDetail = selectedReleaseId.flatMap {
                    libraryStore.releaseDetails[$0]
                }

                if let selectedReleaseId, let selectedDetail {
                    AlbumExpansionContent(
                        summary: summary,
                        selectedRelease: selectedDetail,
                        // Every gallery item carries the whole `BridgeGallery-
                        // Source`; the lightbox passes it to `fetchGalleryBytes`,
                        // which dispatches the read in core. The UI never inspects
                        // the source to pick a fetch.
                        lightboxItems: selectedDetail.galleryItems.map { item in
                            LightboxItem(
                                id: item.id,
                                label: item.label,
                                source: .gallery(
                                    releaseId: selectedReleaseId,
                                    source: item.source
                                )
                            )
                        },
                        releaseCursor: releaseCursorBinding(summary: summary),
                        currentTrackId: playbackStore.nowPlaying.track?.trackId,
                        loadingTrackId: playbackStore.nowPlaying.loadingTrackId,
                        isPlaying: playbackStore.nowPlaying.isPlaying,
                        onClose: { uiStore.closeAlbumDetail() },
                        onPlay: {
                            playback.playRelease(selectedReleaseId, nil, false)
                        },
                        onShuffle: {
                            playback.playRelease(selectedReleaseId, nil, true)
                        },
                        onPlayFromTrack: { index in
                            playback.playRelease(
                                selectedReleaseId,
                                UInt32(index),
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
                        onChangeCover: {
                            presentCoverSheet(
                                summary: summary,
                                selectedReleaseId: selectedReleaseId
                            )
                        },
                        onEditMetadata: {
                            presentEditMetadataSheet(release: selectedDetail)
                        },
                        onReIdentify: {
                            presentReIdentifySheet(release: selectedDetail)
                        },
                        onManage: { presentManageReleaseSheet(selectedDetail) },
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
                            showingDeleteConfirmation = true
                        },
                        onExportTrack: { trackId in
                            exportTrack(trackId: trackId)
                        },
                    )
                }
                else {
                    detailPlaceholder(releaseId: selectedReleaseId)
                }
            }
            else {
                detailPlaceholder(releaseId: nil)
            }
        }
        .task(id: albumId) {
            // Eagerly load details for every release in the album so the
            // release picker has labels and switching between releases
            // doesn't flash a spinner. N is typically 1-3.
            //
            // If the user switches albums mid-loop the outer `.task(id:)`
            // cancels; bail before kicking off the next fetch so we don't
            // populate `releaseDetails` for the album we just left.
            guard let summary = libraryStore.albumSummaries[albumId] else {
                return
            }
            for releaseId in summary.releaseIds {
                if Task.isCancelled {
                    return
                }
                await libraryStore.loadReleaseDetail(
                    releaseId: releaseId,
                    library: library
                )
            }
        }
        .onDisappear {
            uiStore.dismissModal()
            exportTask?.cancel()
            storageTask?.cancel()
            changeCoverTask?.cancel()
            coverSheetTask?.cancel()
        }
        .errorAlert("Cover Change Failed", message: $coverChangeError)
        .errorAlert("Transfer Failed", message: $transferError)
        .errorAlert("Export Failed", message: $exportError)
        .errorAlert("Delete Failed", message: $deleteError)
        .alert("Delete Release", isPresented: $showingDeleteConfirmation) {
            Button("Delete", role: .destructive) {
                guard let releaseId = releaseIdPendingDelete else { return }
                releaseIdPendingDelete = nil
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
                        deleteError = error.displayLine
                    }
                }
            }
            Button("Cancel", role: .cancel) { releaseIdPendingDelete = nil }
        } message: {
            Text(
                "Are you sure you want to delete this release? This cannot be undone."
            )
        }
    }

    // MARK: - Data helpers

    /// The placeholder shown before a release's detail loads: an error + Retry
    /// once its on-demand load has failed, otherwise a spinner. Retry re-runs
    /// the load for that release.
    @ViewBuilder
    private func detailPlaceholder(releaseId: String?) -> some View {
        if let releaseId,
            let error = libraryStore.releaseDetailErrors[releaseId]
        {
            LoadFailureView(line: error.line) {
                Task {
                    await libraryStore.loadReleaseDetail(
                        releaseId: releaseId,
                        library: library
                    )
                }
            }
        }
        else {
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    /// The active release ID for this album. Priority: explicit per-album
    /// selection → album's primary release → first release in
    /// `summary.releaseIds`.
    private func activeReleaseId(summary: AlbumSummary) -> String? {
        if let id = uiStore.selectedReleaseIdByAlbum[albumId],
            summary.releaseIds.contains(id)
        {
            return id
        }
        if summary.releaseIds.contains(summary.primaryReleaseId) {
            return summary.primaryReleaseId
        }
        return summary.releaseIds.first
    }

    private func releaseCursorBinding(summary: AlbumSummary) -> Binding<
        Cursor<ReleaseRef>
    > {
        Binding(
            get: {
                let items = summary.releaseIds.map { ReleaseRef(id: $0) }
                let preferringId = activeReleaseId(summary: summary)
                // The body gates this view on `if let selectedReleaseId`, which
                // is non-nil iff `summary.releaseIds` is non-empty.
                guard
                    let cursor = Cursor(items: items, preferring: preferringId)
                else {
                    fatalError(
                        "releaseIds empty despite parent's selectedReleaseId gate"
                    )
                }
                return cursor
            },
            set: { newCursor in
                let newId = newCursor.current.id
                // "Default" is the primary release — if the user selects that,
                // clear the override instead of storing a redundant entry.
                if newId == summary.primaryReleaseId {
                    uiStore.clearSelectedRelease(inAlbum: albumId)
                }
                else {
                    uiStore.selectRelease(newId, inAlbum: albumId)
                }
                // Lazy-load detail for newly-selected release if not cached.
                Task {
                    await libraryStore.loadReleaseDetail(
                        releaseId: newId,
                        library: library
                    )
                }
            },
        )
    }
}

// MARK: - Modal presenters

extension AlbumDetailView {
    private func presentCoverSheet(
        summary: AlbumSummary,
        selectedReleaseId: String
    ) {
        let releaseEditor = releaseEditor
        coverSheetTask?.cancel()
        coverSheetTask = Task { @MainActor in
            do {
                var releaseImages: [ReleaseImageOption] = []
                for image in summary.releaseIds
                    .compactMap({ libraryStore.releaseDetails[$0] })
                    .flatMap(\.imageFiles)
                {
                    releaseImages.append(
                        ReleaseImageOption(
                            id: image.id,
                            name: image.originalFilename,
                            path: try await mediaPaths.filePath(image.id)
                        )
                    )
                }
                uiStore.presentModal {
                    CoverSheetView(
                        releaseImages: releaseImages,
                        fetchRemoteCovers: {
                            try await releaseEditor.fetchRemoteCovers(
                                selectedReleaseId
                            )
                        },
                        onSelectRemote: { cover in
                            changeCover(
                                albumId: albumId,
                                releaseId: selectedReleaseId,
                                selection: cover.coverChoice.selection,
                            )
                        },
                        onSelectReleaseImage: { fileId in
                            changeCover(
                                albumId: albumId,
                                releaseId: selectedReleaseId,
                                selection: .releaseImage(fileId: fileId),
                            )
                        },
                        onDone: { uiStore.dismissModal() },
                    )
                    .frame(width: 500, height: 450)
                    .background(Theme.background)
                }
            }
            catch is CancellationError {}
            catch {
                coverChangeError = error.displayLine
            }
        }
    }

    private func presentEditMetadataSheet(release: ReleaseDetail) {
        let releaseId = release.id
        // bae-core projects the release's current metadata into the raw form
        // (current state → wire edit → raw form); the UI never shapes it. The
        // seed loads off the release id, so nothing reads the @Observable
        // release/summary fields in the modal builder's render context.
        let releaseEditor = releaseEditor
        Task {
            do {
                let initialForm = try await releaseEditor.seedReleaseEdit(
                    releaseId
                )
                uiStore.presentModal {
                    EditMetadataSheet(
                        initialForm: initialForm,
                        onSave: { edit in
                            try await releaseEditor
                                .updateReleaseMetadataUserEdit(releaseId, edit)
                            uiStore.dismissModal()
                        },
                        onReset: {
                            let edit =
                                try await releaseEditor.resetMetadataToSource(
                                    releaseId
                                )
                            return rawReleaseEditFromUserEdit(
                                edit: edit,
                                trackIdPrefix: "reset-track"
                            )
                        },
                        onCancel: { uiStore.dismissModal() },
                    )
                }
            }
            catch {
                uiStore.showError(error)
            }
        }
    }

    private func presentReIdentifySheet(release: ReleaseDetail) {
        let releaseId = release.id
        let displayName = release.displayName
        let trackCount = UInt32(release.tracks.count)
        uiStore.presentModal {
            ReIdentifySheet(
                releaseId: releaseId,
                displayName: displayName,
                trackCount: trackCount,
                onClose: { uiStore.dismissModal() },
            )
        }
    }

    private func presentManageReleaseSheet(_ detail: ReleaseDetail) {
        uiStore.presentModal {
            ManageReleaseSheet(
                release: detail,
                onAction: { action in
                    performStorageAction(action, releaseId: detail.id)
                },
                onExport: { exportRelease(releaseId: detail.id) },
                onSaveAs: { saveReleaseAs(releaseId: detail.id) },
                onDone: { uiStore.dismissModal() },
            )
            .frame(width: 640, height: 600)
            .background(Theme.background)
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
            unpinRelease(releaseId: releaseId)
        case .makeRemote:
            presentManageConfirmSheet(releaseId: releaseId)
        case .makeLocal:
            unmanageRelease(releaseId: releaseId)
        }
    }

    /// Export a release verbatim to a folder — a pure copy-out that reproduces
    /// the imported file set. Changes no state, so it's offered regardless of the
    /// release's locality. The copy runs on the output queue and surfaces in the
    /// Storage Manager's Exporting pane.
    private func exportRelease(releaseId: String) {
        guard let targetDir = OutputTarget.resolveExportDir() else {
            return
        }
        Task {
            do {
                try await outputs.enqueueExport(releaseId, targetDir)
            }
            catch {
                exportError = error.displayLine
            }
        }
    }

    /// Save a release under a chosen preset + folder — a rendered workup (decode,
    /// encode, tags, cover) rather than a verbatim copy. Runs on the same output
    /// queue as export.
    private func saveReleaseAs(releaseId: String) {
        guard
            let target = OutputTarget.resolveReleaseSave(
                config: configStore.config
            )
        else {
            return
        }
        Task {
            do {
                try await outputs.enqueueReleaseSave(
                    releaseId,
                    target.targetDir,
                    target.presetId
                )
            }
            catch {
                exportError = error.displayLine
            }
        }
    }

    private func presentManageConfirmSheet(releaseId: String) {
        uiStore.presentModal {
            ManageConfirmSheet(
                onConfirm: { pin in
                    manageRelease(releaseId: releaseId, pin: pin)
                },
                onCancel: { uiStore.dismissModal() },
            )
            .frame(width: 420)
            .background(Theme.background)
        }
    }

    // MARK: - Actions

    private func changeCover(
        albumId: String,
        releaseId: String,
        selection: BridgeCoverSelection
    ) {
        let releaseEditor = releaseEditor
        let library = library
        changeCoverTask?.cancel()
        changeCoverTask = Task {
            do {
                try await releaseEditor.changeCover(
                    albumId,
                    releaseId,
                    selection
                )
                uiStore.dismissModal()
                await libraryStore.reloadReleaseDetail(
                    releaseId: releaseId,
                    library: library
                )
            }
            catch is CancellationError {
                // view dismissed mid-cover-change
            }
            catch {
                coverChangeError = error.displayLine
            }
        }
    }

    /// Run one storage transition with the shared cancel-prior /
    /// reload-then-error plumbing. `transition` is the single bridge call that
    /// differs per action. The bridge call is async (it descends into the cloud
    /// future chain on a runtime worker); progress is rendered from the core
    /// `ReleaseTransferProgress` events on the release summary, not from local
    /// view state.
    private func runStorageTransition(
        releaseId: String,
        _ transition: @escaping @Sendable () async throws -> Void
    ) {
        transferError = nil
        let library = library
        storageTask?.cancel()
        storageTask = Task {
            do {
                try await transition()
                await libraryStore.reloadReleaseDetail(
                    releaseId: releaseId,
                    library: library
                )
            }
            catch is CancellationError {
                // View dismissed (or the action re-triggered) mid-transfer.
                // Cancelling this task aborts the Rust transfer future; core's
                // drop guard emits the terminal `ReleaseTransferEnded`, which
                // clears the indicator through the normal event path. Swallow
                // the cancellation so it doesn't surface as a transfer error.
            }
            catch {
                transferError = error.displayLine
            }
        }
    }

    private func pinRelease(releaseId: String) {
        // Pinning enqueues on the in-memory download queue rather than awaiting
        // a per-release transition. The sheet's existing `release.summary.transfer`
        // bar still tracks progress (driven by `ReleaseTransferProgress`); the
        // storage state flips when the worker invalidates the release on
        // completion, so there's no reload to await here.
        Task { await downloads.queuePins([releaseId]) }
    }

    private func unpinRelease(releaseId: String) {
        let downloads = downloads
        runStorageTransition(releaseId: releaseId) {
            try await downloads.unpinRelease(releaseId)
        }
    }

    private func manageRelease(releaseId: String, pin: Bool) {
        uiStore.dismissModal()
        let releaseEditor = releaseEditor
        runStorageTransition(releaseId: releaseId) {
            try await releaseEditor.manageRelease(releaseId, pin)
        }
    }

    private func unmanageRelease(releaseId: String) {
        guard
            let newPath = StorageActionRunner.promptUnmanageDestination()
        else {
            return
        }
        let releaseEditor = releaseEditor
        runStorageTransition(releaseId: releaseId) {
            try await releaseEditor.unmanageRelease(releaseId, newPath)
        }
    }

    private func setPrimaryRelease(releaseId: String) {
        let releaseEditor = releaseEditor
        let albumId = albumId
        Task.detached {
            do {
                try await releaseEditor.setPrimaryRelease(albumId, releaseId)
            }
            catch {
                await MainActor.run {
                    uiStore.showError(
                        String(
                            localized:
                                "Failed to set primary release: \(error.displayLine)"
                        )
                    )
                }
            }
        }
    }

    /// Core renders each preset's suggested stem from its own token pattern;
    /// a failure surfaces rather than falling back to a raw title. Every
    /// preset pre-renders up front, paired with its choice, so the format
    /// popup can swap the stem synchronously during the modal panel (async
    /// work can't run during `runModal`). Nil means the export stops — the
    /// failure already surfaced (or was a cancellation).
    private func renderedSaveChoices(
        trackId: String,
        choices: [SaveFormatChoice]
    ) async -> [TrackSaveChoice]? {
        var saveChoices: [TrackSaveChoice] = []
        for choice in choices {
            let stem: String
            do {
                stem = try await export.suggestedName(trackId, choice.presetId)
            }
            catch is CancellationError {
                return nil
            }
            catch {
                exportError = error.displayLine
                return nil
            }
            saveChoices.append(
                TrackSaveChoice(choice: choice, suggestedStem: stem)
            )
        }
        return saveChoices
    }

    private func exportTrack(trackId: String) {
        exportTask?.cancel()
        exportTask = Task {
            let choices = SaveFormatChoice.trackChoices(
                presets: configStore.config.savePresets
            )
            guard
                let selectedIndex = choices.firstIndex(where: {
                    $0.presetId == configStore.config.defaultTrackSavePreset
                })
            else {
                exportError = String(localized: "Default format")
                return
            }

            guard
                let saveChoices = await renderedSaveChoices(
                    trackId: trackId,
                    choices: choices
                )
            else { return }

            let panel = TrackSavePanel.make(
                saveChoices: saveChoices,
                selectedIndex: selectedIndex
            )
            let formatPopup = panel.formatPopup
            let response = panel.savePanel.runModal()
            _ = panel.formatDelegate  // prevent deallocation during modal
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

            let presetId =
                saveChoices[formatPopup.indexOfSelectedItem].choice.presetId
            let outputPath = url.path(percentEncoded: false)
            do {
                try await export.saveTrack(trackId, outputPath, presetId)
            }
            catch is CancellationError {
                // view dismissed mid-save; OutputFileGuard cleaned up
            }
            catch {
                exportError = error.displayLine
            }
        }
    }
}

extension View {
    /// Presents an OK-dismissible alert bound to an optional error message,
    /// clearing it on dismiss. Collapses the repeated "alert over a `String?`
    /// @State error" pattern.
    fileprivate func errorAlert(
        _ title: LocalizedStringKey,
        message: Binding<String?>
    )
        -> some View
    {
        alert(
            title,
            isPresented: Binding(
                get: { message.wrappedValue != nil },
                set: { if !$0 { message.wrappedValue = nil } },
            )
        ) {
            Button("OK") { message.wrappedValue = nil }
        } message: {
            if let err = message.wrappedValue {
                Text(err)
            }
        }
    }
}

#if DEBUG
    #Preview("Album Detail — Single Disc") {
        AlbumDetailView(albumId: "a-01")
            .frame(width: 1100, height: 780)
            .background(Theme.background)
            .albumDetailPreviewEnvironment(
                store: PreviewData.seededLibraryStore()
            )
            .preferredColorScheme(.dark)
    }

    #Preview("Album Detail — Multiple Releases") {
        AlbumDetailView(albumId: "a-04")
            .frame(width: 1100, height: 780)
            .background(Theme.background)
            .albumDetailPreviewEnvironment(
                store: PreviewData.seededLibraryStore()
            )
            .preferredColorScheme(.dark)
    }
#endif
