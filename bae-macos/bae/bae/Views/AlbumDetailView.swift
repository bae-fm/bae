import AppKit
import BaeKit
import OSLog
import SwiftUI
import UniformTypeIdentifiers

// MARK: - ReleaseRef

/// Identifiable wrapper around a release id, for use with `Cursor`. The id
/// is the only field — `String` itself isn't `Identifiable` in stdlib and
/// retroactive conformance is fragile.
struct ReleaseRef: Identifiable, Equatable {
    let id: String
}

private struct ExportSavePanel {
    let savePanel: NSSavePanel
    let formatPopup: NSPopUpButton
    let formatDelegate: ExportFormatDelegate
}

/// A track-save format choice paired with the stem its preset's pattern
/// suggests. Built together in one pass, so the stem is never absent — the
/// track panel and its format delegate read it directly, no lookup or fallback.
/// (The release flow doesn't pre-render stems, so this pairing is panel-local
/// rather than a field on the shared `ExportFormatChoice`.)
private struct TrackSaveChoice {
    let choice: ExportFormatChoice
    let suggestedStem: String
}

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
    @Environment(Exports.self)
    var exports
    @Environment(ConfigStore.self)
    var configStore
    @Environment(Export.self)
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
        guard let targetDir = ExportTarget.resolveExportDir() else {
            return
        }
        Task {
            do {
                try await exports.enqueueExport(releaseId, targetDir)
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
            let target = ExportTarget.resolveReleaseSave(
                config: configStore.config
            )
        else {
            return
        }
        Task {
            do {
                try await exports.enqueueReleaseSave(
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

    private func exportTrack(trackId: String) {
        exportTask?.cancel()
        exportTask = Task {
            let choices = ExportFormatChoice.trackChoices(
                presets: configStore.config.exportPresets
            )
            guard
                let selectedIndex = choices.firstIndex(where: {
                    $0.presetId == configStore.config.defaultTrackSavePreset
                })
            else {
                exportError = String(localized: "Default format")
                return
            }

            // Core renders each preset's suggested stem from its own token
            // pattern; a failure surfaces rather than falling back to a raw
            // title. Pre-render every preset up front, paired with its choice,
            // so the format popup can swap the stem synchronously during the
            // modal panel (async work can't run during `runModal`).
            var saveChoices: [TrackSaveChoice] = []
            for choice in choices {
                let stem: String
                do {
                    stem = try await export.suggestedName(
                        trackId,
                        choice.presetId
                    )
                }
                catch is CancellationError {
                    return
                }
                catch {
                    exportError = error.displayLine
                    return
                }
                saveChoices.append(
                    TrackSaveChoice(choice: choice, suggestedStem: stem)
                )
            }

            let panel = makeExportSavePanel(
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

    /// Build the save panel for a track save, with its format-picker accessory
    /// wired to a delegate that keeps the extension and the suggested stem in
    /// sync as the preset changes. Seeded from the default preset's suggestion;
    /// the caller runs the panel and reads `formatPopup` for the chosen preset.
    /// The delegate must be retained for the modal's lifetime.
    private func makeExportSavePanel(
        saveChoices: [TrackSaveChoice],
        selectedIndex: Int
    ) -> ExportSavePanel {
        let selected = saveChoices[selectedIndex]
        let stem = selected.suggestedStem

        let panel = NSSavePanel()
        panel.nameFieldStringValue = "\(stem).\(selected.choice.extensionName)"
        panel.canCreateDirectories = true

        // Format picker accessory: swaps extension and re-suggests the stem on
        // change (the delegate replaces it only when the user hasn't edited it).
        let formatDelegate = ExportFormatDelegate(
            panel: panel,
            saveChoices: saveChoices,
            lastSuggestion: stem
        )
        let formatPopup = NSPopUpButton(
            frame: NSRect(x: 0, y: 0, width: 200, height: 24),
            pullsDown: false
        )
        formatPopup.addItems(withTitles: saveChoices.map(\.choice.title))
        formatPopup.selectItem(at: selectedIndex)
        formatPopup.target = formatDelegate
        formatPopup.action = #selector(ExportFormatDelegate.formatChanged(_:))

        let accessoryContainer = NSView(
            frame: NSRect(x: 0, y: 0, width: 250, height: 34)
        )
        let label = NSTextField(labelWithString: String(localized: "Format:"))
        label.frame = NSRect(x: 0, y: 7, width: 50, height: 20)
        formatPopup.frame = NSRect(x: 54, y: 5, width: 190, height: 24)
        accessoryContainer.addSubview(label)
        accessoryContainer.addSubview(formatPopup)
        panel.accessoryView = accessoryContainer

        if let type = UTType(filenameExtension: selected.choice.extensionName) {
            panel.allowedContentTypes = [type]
        }

        return ExportSavePanel(
            savePanel: panel,
            formatPopup: formatPopup,
            formatDelegate: formatDelegate
        )
    }
}

// MARK: - AlbumExpansionContent

struct AlbumExpansionContent: View {
    let summary: AlbumSummary
    /// Fat detail for the release the user is currently viewing.
    let selectedRelease: ReleaseDetail
    let lightboxItems: [LightboxItem]
    /// Cursor over the album's releases. Drives the release picker and
    /// guarantees a valid selection on every read.
    @Binding
    var releaseCursor: Cursor<ReleaseRef>
    let currentTrackId: String?
    /// The id of the track currently loading (cloud download / decode warm-up),
    /// or nil. The matching row shows a spinner where its play/speaker glyph goes.
    let loadingTrackId: String?
    let isPlaying: Bool
    let onClose: () -> Void
    let onPlay: () -> Void
    let onShuffle: () -> Void
    let onPlayFromTrack: (Int) -> Void
    let onTogglePlayPause: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onAddNextAlbum: () -> Void
    let onAddAlbumToQueue: () -> Void
    let onChangeCover: () -> Void
    let onEditMetadata: () -> Void
    let onReIdentify: () -> Void
    let onManage: () -> Void
    let onExportRelease: () -> Void
    let onSaveReleaseAs: () -> Void
    let onSetPrimaryRelease: () -> Void
    let onDeleteRelease: () -> Void
    let onExportTrack: (String) -> Void

    @Environment(LibraryStore.self)
    private var libraryStore
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: 36) {
                albumArt
                    .frame(width: 340, height: 340)
                    .clipShape(RoundedRectangle(cornerRadius: 14))
                    .shadow(color: .black.opacity(0.6), radius: 20, y: 12)
                    .contentShape(Rectangle())
                    .onTapGesture {
                        if !lightboxItems.isEmpty {
                            uiStore.presentLightbox(items: lightboxItems)
                        }
                    }
                VStack(alignment: .leading, spacing: 4) {
                    Text(summary.title)
                        .font(.system(size: 30, weight: .heavy))
                        .tracking(-0.5)
                        .lineLimit(1)
                    HStack(spacing: 8) {
                        Text(summary.artistNames)
                            .foregroundStyle(.secondary)
                        if let year = summary.year {
                            Text(verbatim: "\u{00B7}")
                                .foregroundStyle(.tertiary)
                            Text(String(year))
                                .foregroundStyle(.tertiary)
                        }
                    }
                    .font(.system(size: 15, weight: .semibold))
                    .lineLimit(1)
                    if releaseCursor.canCycle {
                        releasePicker
                    }
                    Text(selectedRelease.compactMetadata)
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(.tertiary)
                    HStack(spacing: 10) {
                        Button(action: onPlay) {
                            HStack(spacing: 7) {
                                Image(systemName: "play.fill")
                                    .font(.system(size: 12, weight: .bold))
                                Text("Play")
                                    .font(.system(size: 15, weight: .bold))
                            }
                            .foregroundStyle(.white)
                            .padding(.horizontal, 20)
                            .padding(.vertical, 9)
                            .background(
                                Theme.accent,
                                in: RoundedRectangle(cornerRadius: 10)
                            )
                            .shadow(
                                color: Theme.accent.opacity(0.22),
                                radius: 5,
                                y: 3
                            )
                        }
                        .buttonStyle(.plain)
                        albumMenu
                    }
                    .padding(
                        EdgeInsets(top: 12, leading: 0, bottom: 0, trailing: 0)
                    )
                    AlbumTrackListView(
                        release: selectedRelease,
                        isCompilation: summary.isCompilation,
                        currentTrackId: currentTrackId,
                        loadingTrackId: loadingTrackId,
                        isPlaying: isPlaying,
                        onPlayFromTrack: onPlayFromTrack,
                        onTogglePlayPause: onTogglePlayPause,
                        onAddNext: onAddNext,
                        onAddToQueue: onAddToQueue,
                        onExportTrack: onExportTrack,
                    )
                    .padding(.top, 18)
                }
            }
        }
        .padding(32)
        .background(
            LinearGradient(
                colors: [Theme.surfaceElevated, Theme.surface],
                startPoint: .top,
                endPoint: .bottom
            ),
            in: RoundedRectangle(cornerRadius: 18)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 18)
                .strokeBorder(.white.opacity(0.08), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.45), radius: 28, y: 18)
        .overlay(alignment: .topTrailing) {
            Button {
                withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) {
                    onClose()
                }
            } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: 30, height: 30)
                    .background(
                        .white.opacity(0.06),
                        in: RoundedRectangle(cornerRadius: 9)
                    )
            }
            .buttonStyle(.plain)
            .padding(16)
        }
    }

    private var canSetAsPrimaryRelease: Bool {
        selectedRelease.id != summary.primaryReleaseId
    }

    private var albumMenu: some View {
        Menu {
            Button(action: onPlay) {
                Label("Play", systemImage: "play.fill")
            }
            Button(action: onShuffle) {
                Label("Shuffle", systemImage: "shuffle")
            }
            Divider()
            Button(action: { onAddNextAlbum() }) {
                Label(
                    "Play Next",
                    systemImage: "text.line.first.and.arrowtriangle.forward"
                )
            }
            Button(action: { onAddAlbumToQueue() }) {
                Label("Add to Queue", systemImage: "text.append")
            }
            Divider()
            Button("Change Cover...") { onChangeCover() }
            Button("Edit metadata...") { onEditMetadata() }
            Button("Re-identify...") { onReIdentify() }
            Button("Storage...") { onManage() }
            Button("Export…") { onExportRelease() }
            Button("Save As…") { onSaveReleaseAs() }
            if releaseCursor.canCycle, canSetAsPrimaryRelease {
                Divider()
                Button("Set as Primary Release") { onSetPrimaryRelease() }
            }
            Divider()
            Button(role: .destructive, action: onDeleteRelease) {
                Label("Delete Release", systemImage: "trash")
            }
        } label: {
            Image(systemName: "ellipsis")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(.secondary)
                .frame(width: 36, height: 36)
                .background(
                    .white.opacity(0.07),
                    in: RoundedRectangle(cornerRadius: 10)
                )
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
    }

    private var albumArt: some View {
        ImageView(imageRef: selectedRelease.summary.cover, pointSize: 340)
    }

    private var releasePicker: some View {
        NativeSegmentedControl(
            selectedIndex: Binding(
                get: { releaseCursor.index },
                set: { newIndex in
                    // Guard against an out-of-range index from AppKit's segmented
                    // control. `Cursor.select(id:)` is a no-op for unknown ids.
                    guard releaseCursor.items.indices.contains(newIndex) else {
                        return
                    }
                    releaseCursor.select(id: releaseCursor.items[newIndex].id)
                },
            ),
            segments: releaseCursor.items.enumerated()
                .map { index, ref in
                    NativeSegmentedControl.Segment(
                        label: libraryStore.releaseDetails[ref.id]?.displayName
                            ?? String(localized: "Release \(index + 1)"),
                        systemImage: releaseContainsCurrentTrack(id: ref.id)
                            ? "speaker.fill" : nil,
                    )
                },
        )
        .padding(EdgeInsets(top: 4, leading: 0, bottom: 2, trailing: 0))
    }

    private func releaseContainsCurrentTrack(id: String) -> Bool {
        guard let currentTrackId else {
            return false
        }
        guard let detail = libraryStore.releaseDetails[id] else {
            return false
        }
        return detail.tracks.contains(where: { $0.id == currentTrackId })
    }
}

// MARK: - AlbumTrackListView

struct AlbumTrackListView: View {
    let release: ReleaseDetail
    let isCompilation: Bool
    let currentTrackId: String?
    let loadingTrackId: String?
    let isPlaying: Bool
    let onPlayFromTrack: (Int) -> Void
    let onTogglePlayPause: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onExportTrack: (String) -> Void

    @ScaledMetric(relativeTo: .body)
    private var rowHeight: CGFloat = 40
    @ScaledMetric(relativeTo: .body)
    private var rowHeightCompilation: CGFloat = 52

    var body: some View {
        let groups = release.trackGroups
        var runningOffset = 0
        let offsets = groups.map { group -> Int in
            let offset = runningOffset
            runningOffset += group.tracks.count
            return offset
        }

        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(groups.enumerated()), id: \.offset) {
                groupIndex,
                group in
                let globalOffset = offsets[groupIndex]
                if !group.sideHeaderText.isEmpty {
                    Text(group.sideHeaderText)
                        .font(.system(size: 10, weight: .bold))
                        .tracking(1.2)
                        .textCase(.uppercase)
                        .foregroundStyle(.secondary)
                        .padding(.top, groupIndex == 0 ? 0 : 18)
                        .padding(.bottom, 6)
                }
                if group.tracks.count > 8 {
                    let mid = (group.tracks.count + 1) / 2
                    let left = Array(group.tracks.prefix(mid))
                    let right = Array(group.tracks.dropFirst(mid))
                    HStack(alignment: .top, spacing: 40) {
                        trackColumn(tracks: left, globalOffset: globalOffset)
                        trackColumn(
                            tracks: right,
                            globalOffset: globalOffset + mid
                        )
                    }
                }
                else {
                    trackColumn(
                        tracks: group.tracks,
                        globalOffset: globalOffset
                    )
                }
            }
            if !release.totalDurationText.isEmpty {
                Text(release.totalDurationText)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.tertiary)
                    .padding(.top, 14)
            }
        }
    }

    private func trackColumn(tracks: [Track], globalOffset: Int) -> some View {
        let height = isCompilation ? rowHeightCompilation : rowHeight
        return VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(tracks.enumerated()), id: \.element.id) {
                localIndex,
                track in
                TrackRowView(
                    track: track,
                    // Core's decision (set only for a compilation); the row does
                    // not re-derive it. `isCompilation` above is kept only for the
                    // album-level row-height heuristic.
                    artist: track.displayArtist,
                    isCurrent: currentTrackId == track.id,
                    isLoading: loadingTrackId == track.id,
                    isPlaying: isPlaying,
                    onPlay: { onPlayFromTrack(globalOffset + localIndex) },
                    onTogglePlayPause: onTogglePlayPause,
                    onAddNext: onAddNext,
                    onAddToQueue: onAddToQueue,
                    onExportTrack: onExportTrack,
                )
                .id(track.id)
                .frame(height: height)
            }
        }
    }
}

private struct TrackRowView: View {
    @Environment(UiStore.self)
    private var uiStore
    let track: Track
    /// The artist to show, or `nil` for none — core's decision.
    let artist: String?
    let isCurrent: Bool
    let isLoading: Bool
    let isPlaying: Bool
    let onPlay: () -> Void
    let onTogglePlayPause: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onExportTrack: (String) -> Void

    @State
    private var isHovered = false
    @State
    private var hoverWorkItem: DispatchWorkItem?
    @State
    private var highlightOpacity: Double = 0

    var body: some View {
        let isCurrentPlaying = isCurrent && isPlaying
        HStack(spacing: 14) {
            // One leading slot carries the track number and everything that
            // stands in for it — the hover play/pause, the playing speaker,
            // the loading spinner. All stay in the layout tree, opacity-
            // toggled, so the row's intrinsic size never changes and sibling
            // rows don't re-measure.
            ZStack {
                trackNumberLabel
                    .font(.system(size: 13, weight: .medium).monospacedDigit())
                    .foregroundStyle(.tertiary)
                    .opacity(!isCurrent && !isHovered && !isLoading ? 1 : 0)

                Button(action: isCurrent ? onTogglePlayPause : onPlay) {
                    Image(
                        systemName: isCurrentPlaying
                            ? "pause.fill" : "play.fill"
                    )
                    .font(.system(size: 12, weight: .semibold))
                }
                .buttonStyle(.plain)
                .opacity(isHovered && !isLoading ? 1 : 0)
                .allowsHitTesting(isHovered && !isLoading)

                Image(systemName: "speaker.wave.2.fill")
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.accent)
                    .opacity(isCurrent && !isHovered && !isLoading ? 1 : 0)

                ProgressView()
                    .controlSize(.small)
                    .opacity(isLoading ? 1 : 0)
                    .allowsHitTesting(isLoading)
            }
            .frame(width: 22)
            VStack(alignment: .leading, spacing: 2) {
                Text(track.title)
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(isCurrent ? Theme.accent : .primary)
                    .lineLimit(1)
                if let artist {
                    Text(artist)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 0)
            if !track.durationLabel.isEmpty {
                Text(track.durationLabel)
                    .font(
                        .system(size: 12.5, weight: .medium).monospacedDigit()
                    )
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 10)
        .frame(maxHeight: .infinity)
        .background(
            ZStack {
                RoundedRectangle(cornerRadius: 8)
                    .fill(.white.opacity(isHovered ? 0.05 : 0))
                RoundedRectangle(cornerRadius: 8)
                    .fill(Theme.accent.opacity(highlightOpacity))
            }
        )
        // The row chrome (the hover fill) bleeds past the text column so the
        // content stays aligned with the header block above the list.
        .padding(.horizontal, -10)
        // Keyed on the flash's `seq` (not a subject) for the same reason the
        // grid scroll is: navigating here can remount this row, and durable
        // state survives that where a one-shot emit would be lost. Re-fires when
        // seq changes (repeat navigation). Independent of the grid-scroll
        // consumer sharing the same navigateToAlbum call — each clears only its
        // own pending command, so one consumer can't starve the other.
        .task(id: uiStore.pendingTrackFlash?.seq) {
            guard let flash = uiStore.pendingTrackFlash,
                flash.trackId == track.id
            else {
                return
            }
            highlightOpacity = 0.3
            withAnimation(.easeOut(duration: 3)) {
                highlightOpacity = 0
            }
            uiStore.consumeTrackFlash(seq: flash.seq)
        }
        .onTapGesture(count: 2) {
            onPlay()
        }
        .contentShape(Rectangle())
        .onHover { hovering in
            hoverWorkItem?.cancel()
            if hovering {
                let item = DispatchWorkItem { isHovered = true }
                hoverWorkItem = item
                DispatchQueue.main.asyncAfter(
                    deadline: .now() + 0.05,
                    execute: item
                )
            }
            else {
                isHovered = false
                hoverWorkItem = nil
            }
        }
        .contextMenu {
            Button("Play") { onPlay() }
            Button("Play Next") { onAddNext(track.id) }
            Button("Add to Queue") { onAddToQueue(track.id) }
            Divider()
            Button("Save As…") { onExportTrack(track.id) }
        }
        .draggable(track.id)
    }

    private var trackNumberLabel: some View {
        Text(track.positionText)
    }
}

// MARK: - ManageReleaseSheet

struct ManageReleaseSheet: View {
    let release: ReleaseDetail
    let onAction: (BridgeReleaseStorageAction) -> Void
    let onExport: () -> Void
    let onSaveAs: () -> Void
    let onDone: () -> Void

    /// Column the file table sorts by. Defaults to filename; the user clicks a
    /// column header to re-sort. The Format column is display-only (its value is
    /// optional, so there's no natural ordering key) — sort by Kind to group by
    /// content type.
    @State
    private var sortOrder: [KeyPathComparator<BridgeFile>] = [
        KeyPathComparator(\BridgeFile.originalFilename)
    ]

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Storage")
                    .font(.headline)
                Spacer()
                Button("Done") { onDone() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding()
            Divider()
            StorageStatusBand(
                release: release,
                onAction: onAction,
                onExport: onExport,
                onSaveAs: onSaveAs
            )
            Divider()
            filesSection
        }
    }

    private var filesSection: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Files")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Spacer()
            }
            .padding(.horizontal)
            .padding(.top, 8)
            .padding(.bottom, 4)

            Table(release.files.sorted(using: sortOrder), sortOrder: $sortOrder)
            {
                TableColumn("Name", value: \.originalFilename) { file in
                    Text(file.originalFilename).lineLimit(1)
                }
                TableColumn("Format") { file in
                    // Audio files carry a label; non-audio files (images, cue)
                    // have none, so their Format cell is simply empty.
                    if let format = file.audioFormat {
                        Text(format.text)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
                TableColumn("Size", value: \.fileSize) { file in
                    Text(file.fileSizeText)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                .width(min: 70, ideal: 80)
                TableColumn("Kind", value: \.contentType) { file in
                    Text(file.contentType)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                }
                .width(min: 90, ideal: 130)
            }
        }
    }
}

/// Storage state, any in-flight transfer/upload progress, and the available
/// storage actions for a release. A separate leaf so transfer and outbox ticks
/// re-render only this band, not the sheet's file table.
private struct StorageStatusBand: View {
    let release: ReleaseDetail
    let onAction: (BridgeReleaseStorageAction) -> Void
    let onExport: () -> Void
    let onSaveAs: () -> Void
    @Environment(OutboxStore.self)
    private var outboxStore

    /// Outbox progress for this release, if any work is in flight (releases
    /// with nothing left to ship are absent from the per-release map). Drives
    /// the "uploading…" indicator and suppresses transfer actions — acting
    /// mid-upload races the observer that completes the unmanaged → managed
    /// step.
    private var uploadProgress: BridgeUploadProgress? {
        outboxStore.progress(forRelease: release.summary.id)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            storageStatus
            // Read the live transfer state off the identity-stable summary so a
            // running pin/unpin/manage/unmanage updates the bar in place.
            if let transfer = release.summary.transfer {
                progressBar(
                    label: transfer.label
                )
            }
            else if let progress = uploadProgress {
                progressBar(
                    value: progress.fraction,
                    label: String(
                        localized:
                            "Uploading (\(Int(progress.pending)) remaining)…"
                    )
                )
            }
            else {
                transferActions
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
    }

    private func progressBar(value: Double? = nil, label: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            ProgressView(value: value)
                .progressViewStyle(.linear)
            Text(label)
                .font(.callout)
                .foregroundStyle(.secondary)
        }
    }

    private var storageStatus: some View {
        HStack(spacing: 6) {
            switch release.summary.storageState {
            case .local:
                Image(systemName: "folder")
                Text("Unmanaged")
            case .remote:
                if release.summary.pinned {
                    Image(systemName: "pin.fill")
                    Text("Pinned for offline")
                }
                else {
                    Image(systemName: "cloud")
                    Text("Cloud")
                }
            }
        }
        .font(.callout)
        .foregroundStyle(.secondary)
    }

    private var transferActions: some View {
        Group {
            ForEach(release.storageActions, id: \.self) { action in
                Button(action: { onAction(action) }) {
                    Label(action.label, systemImage: action.systemImage)
                }
            }
            // Export (verbatim) and Save As (preset workup) are pure outputs —
            // no state change — so both are offered for every release regardless
            // of locality, not among the core-computed `storageActions`.
            Button(action: { onExport() }) {
                Label("Export…", systemImage: "square.and.arrow.up")
            }
            Button(action: { onSaveAs() }) {
                Label("Save As…", systemImage: "square.and.arrow.down")
            }
        }
    }
}

// MARK: - ManageConfirmSheet

/// Confirmation for moving an unmanaged release into the library. The single
/// toggle maps to the `manage_release` pin option: whether to keep a local copy
/// (pinned for offline) once the release is managed.
struct ManageConfirmSheet: View {
    let onConfirm: (_ pin: Bool) -> Void
    let onCancel: () -> Void

    @State
    private var pin: Bool = true

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Move into library")
                .font(.headline)
            Toggle("Pin for offline", isOn: $pin)
            HStack {
                Spacer()
                Button("Cancel") { onCancel() }
                    .keyboardShortcut(.cancelAction)
                Button("Move") { onConfirm(pin) }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
    }
}

// MARK: - ExportFormatDelegate

/// Target-action handler for the format popup in the track save panel. On a
/// format change it swaps the filename extension and content type, and
/// re-suggests the stem from the newly selected preset's pattern — but only when
/// the user hasn't edited the stem away from the last suggestion. Each choice
/// carries its pre-rendered stem (async work can't run during `runModal`), so
/// the suggestion is always present.
@MainActor
private class ExportFormatDelegate: NSObject {
    weak var panel: NSSavePanel?
    let saveChoices: [TrackSaveChoice]
    /// The stem the panel currently shows as an unedited suggestion. When the
    /// visible stem still equals this, the user hasn't typed over it, so a
    /// format change is free to replace it with the new preset's suggestion.
    var lastSuggestion: String

    init(
        panel: NSSavePanel,
        saveChoices: [TrackSaveChoice],
        lastSuggestion: String
    ) {
        self.panel = panel
        self.saveChoices = saveChoices
        self.lastSuggestion = lastSuggestion
    }

    @objc
    func formatChanged(_ sender: NSPopUpButton) {
        guard let panel else {
            return
        }
        let selected = saveChoices[sender.indexOfSelectedItem]
        let currentStem =
            (panel.nameFieldStringValue as NSString).deletingPathExtension
        var stem = currentStem
        if currentStem == lastSuggestion {
            stem = selected.suggestedStem
            lastSuggestion = selected.suggestedStem
        }

        panel.nameFieldStringValue = "\(stem).\(selected.choice.extensionName)"
        if let type = UTType(filenameExtension: selected.choice.extensionName) {
            panel.allowedContentTypes = [type]
        }
    }
}

#if DEBUG
    // MARK: - Previews

    @MainActor
    private func previewExpansion(
        albumId: String,
        currentTrackId: String? = nil,
        loadingTrackId: String? = nil,
        isPlaying: Bool = false,
    ) -> some View {
        let store = LibraryStore()
        guard let album = PreviewData.albumDetails[albumId] else {
            fatalError("no preview album for id: \(albumId)")
        }
        store.internAlbumDetail(album)
        guard let summary = store.albumSummaries[albumId] else {
            fatalError(
                "preview seed did not create summary for id: \(albumId)"
            )
        }
        guard let primary = store.releaseDetails[summary.primaryReleaseId]
        else {
            fatalError(
                "no releaseDetail seeded for id: \(summary.primaryReleaseId)"
            )
        }
        let cursor = PreviewData.releaseCursor(
            releaseIds: summary.releaseIds,
            preferring: summary.primaryReleaseId
        )
        return
            PreviewData.albumExpansionContent(
                summary: summary,
                selectedRelease: primary,
                releaseCursor: .constant(cursor),
                currentTrackId: currentTrackId,
                loadingTrackId: loadingTrackId,
                isPlaying: isPlaying,
            )
            .padding()
            .frame(width: 1100)
            .background(Theme.background)
            .environment(UiStore())
            .environment(store)
    }

    #Preview("Single Disc") {
        previewExpansion(
            albumId: "a-01",
            currentTrackId: "t-d1-2",
            isPlaying: true
        )
    }

    #Preview("Single Disc — Track Loading") {
        previewExpansion(
            albumId: "a-01",
            currentTrackId: "t-d1-2",
            loadingTrackId: "t-d1-3",
            isPlaying: true
        )
    }

    #Preview("Vinyl — Two Sides") {
        previewExpansion(
            albumId: "a-21",
            currentTrackId: "t-d2-3",
            isPlaying: true
        )
    }

    #Preview("CD — Two Discs") {
        previewExpansion(albumId: "a-22")
    }

    struct MultiReleasePreview: View {
        @State
        private var selectedReleaseId: String = "rel-a-04-0"
        @State
        private var store = LibraryStore()

        var body: some View {
            Group {
                if let summary = store.albumSummaries["a-04"],
                    let selected = store.releaseDetails[selectedReleaseId]
                {
                    PreviewData.albumExpansionContent(
                        summary: summary,
                        selectedRelease: selected,
                        // Live cursor so selecting a release in the picker cycles
                        // the preview to that release's detail.
                        releaseCursor: Binding(
                            get: {
                                PreviewData.releaseCursor(
                                    releaseIds: summary.releaseIds,
                                    preferring: selectedReleaseId
                                )
                            },
                            set: { selectedReleaseId = $0.current.id },
                        ),
                        currentTrackId: "t-d2-3",
                        isPlaying: true,
                    )
                    .padding()
                    .frame(width: 1100, height: 700, alignment: .top)
                    .background(Theme.background)
                    .environment(UiStore())
                    .environment(store)
                    .environment(MediaPaths.stub)
                }
            }
            .onAppear { seedIfNeeded() }
        }

        @MainActor
        private func seedIfNeeded() {
            if store.albumSummaries["a-04"] == nil {
                guard let album = PreviewData.albumDetails["a-04"] else {
                    fatalError("a-04 not in PreviewData.albumDetails")
                }
                store.internAlbumDetail(album)
            }
        }
    }

    #Preview("Multiple Releases") {
        MultiReleasePreview()
            .environment(MediaPaths.stub)
            .environment(UiStore())
            .environment(LibraryStore())
    }
#endif

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
