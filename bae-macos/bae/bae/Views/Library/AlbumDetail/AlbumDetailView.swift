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
                        // Source`; the lightbox passes it to `fetchReleaseImageBytes`,
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
                                selectedReleaseId: selectedReleaseId
                            )
                        },
                        onEditMetadata: {
                            presentEditMetadataSheet(release: selectedDetail)
                        },
                        onReIdentify: {
                            presentReIdentifySheet(release: selectedDetail)
                        },
                        onOpenStorage: {
                            presentReleaseStorageSheet(selectedDetail)
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
        .onChange(of: albumId, initial: true) { oldId, newId in
            if oldId != newId {
                libraryStore.deactivateAlbumDetail(albumId: oldId)
            }
            libraryStore.activateAlbumDetail(
                albumId: newId,
                library: library
            )
        }
        .onDisappear {
            libraryStore.deactivateAlbumDetail(albumId: albumId)
            uiStore.dismissModal()
            exportTask?.cancel()
            storageTask?.cancel()
        }
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

    /// The placeholder shown before the album detail subscription delivers its
    /// first value. A failed subscription can be replaced from Retry.
    @ViewBuilder
    private func detailPlaceholder(releaseId _: String?) -> some View {
        if let error = libraryStore.albumDetailErrors[albumId] {
            LoadFailureView(line: error.line) {
                libraryStore.retryAlbumDetail(
                    albumId: albumId,
                    library: library
                )
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
            },
        )
    }
}

// MARK: - Modal presenters

extension AlbumDetailView {
    private func presentCoverSheet(selectedReleaseId: String) {
        let releaseEditor = releaseEditor
        uiStore.presentModal {
            CoverPickerFrame {
                CoverSheetView(
                    releaseId: selectedReleaseId,
                    fetchRemoteCovers: {
                        try await releaseEditor.fetchRemoteCovers(
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

    private func presentEditMetadataSheet(release: ReleaseDetail) {
        let releaseId = release.id
        // bae-core projects the release's current metadata into the raw form
        // (current state → wire edit → raw form); the UI never shapes it. The
        // seed loads off the release id, so nothing reads the @Observable
        // release/summary fields in the modal builder's render context.
        let releaseEditor = releaseEditor
        Task {
            do {
                let seed = try await releaseEditor.seedReleaseEdit(
                    releaseId
                )
                uiStore.presentModal {
                    EditMetadataSheet(
                        releaseId: releaseId,
                        seed: seed,
                        onSave: { edit in
                            try await releaseEditor
                                .updateReleaseMetadataUserEdit(releaseId, edit)
                        },
                        onReset: {
                            try await releaseEditor.resetReleaseEditToSource(
                                releaseId
                            )
                        },
                        onSaved: { uiStore.dismissModal() },
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
        uiStore.presentModal {
            ReIdentifySheet(
                releaseId: releaseId,
                displayName: displayName,
                onClose: { uiStore.dismissModal() },
            )
        }
    }

    private func presentReleaseStorageSheet(_ detail: ReleaseDetail) {
        uiStore.presentModal {
            ReleaseStorageSheet(
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
            presentMoveToCloudConfirmSheet(releaseId: releaseId)
        case .makeLocal:
            makeReleaseLocal(releaseId: releaseId)
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

    private func presentMoveToCloudConfirmSheet(releaseId: String) {
        uiStore.presentModal {
            MoveToCloudConfirmSheet(
                onConfirm: { pin in
                    moveReleaseToCloud(releaseId: releaseId, pin: pin)
                },
                onCancel: { uiStore.dismissModal() },
            )
            .frame(width: 420)
            .background(Theme.background)
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
                transferError = error.displayLine
            }
        }
    }

    private func pinRelease(releaseId: String) {
        // Pinning enqueues on the in-memory download queue rather than awaiting
        // a per-release transition. The sheet's existing `release.summary.transfer`
        // bar tracks the action from subscribed release values; the storage state
        // flips when the live album detail delivers the worker's database commit.
        Task { try await downloads.queuePins([releaseId]) }
    }

    private func unpinRelease(releaseId: String) {
        let downloads = downloads
        runStorageTransition {
            try await downloads.unpinRelease(releaseId)
        }
    }

    private func moveReleaseToCloud(releaseId: String, pin: Bool) {
        uiStore.dismissModal()
        let releaseEditor = releaseEditor
        runStorageTransition {
            try await releaseEditor.moveReleaseToCloud(releaseId, pin)
        }
    }

    private func makeReleaseLocal(releaseId: String) {
        guard
            let newPath = StorageActionRunner.promptMakeLocalDestination()
        else {
            return
        }
        let releaseEditor = releaseEditor
        runStorageTransition {
            try await releaseEditor.makeReleaseLocal(releaseId, newPath)
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
                // fault and offer Copy Details.
                guard let displayed = DisplayError(error) else { return }
                uiStore.showError(
                    displayed.addingContext(
                        String(localized: "Failed to set primary release")
                    )
                )
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
        // Bind the seeded store to a `store` local the audit can resolve, so it
        // credits the LibraryStore that albumDetailPreviewEnvironment injects.
        let store = PreviewData.seededLibraryStore()
        AlbumDetailView(albumId: "a-01")
            .frame(width: 1100, height: 780)
            .background(Theme.background)
            .albumDetailPreviewEnvironment(store: store)
            .preferredColorScheme(.dark)
    }

    #Preview("Album Detail — Multiple Releases") {
        let store = PreviewData.seededLibraryStore()
        AlbumDetailView(albumId: "a-04")
            .frame(width: 1100, height: 780)
            .background(Theme.background)
            .albumDetailPreviewEnvironment(store: store)
            .preferredColorScheme(.dark)
    }
#endif

/// Identifiable wrapper around a release id, for use with `Cursor`. The id
/// is the only field — `String` itself isn't `Identifiable` in stdlib and
/// retroactive conformance is fragile.
struct ReleaseRef: Identifiable, Equatable {
    let id: String
}
