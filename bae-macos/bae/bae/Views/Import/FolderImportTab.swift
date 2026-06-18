import AppKit
import OSLog
import SwiftUI

private let logger = Logger.bae("FolderImportTab")

struct FolderImportTab: View {
    @Environment(Importer.self)
    var importer
    @Environment(Library.self)
    var library
    @Environment(PreviewAudio.self)
    var previewAudio
    @Environment(ImportStore.self)
    var importStore
    @Environment(ConfigStore.self)
    var configStore

    @State
    private var selectedKey: String?
    // Last-used storage choice, persisted; only consulted when a cloud home
    // exists (toggle hidden otherwise), Config.importStorageMode forces Unmanaged
    // without one.
    @AppStorage("importStorageManaged")
    private var storageManaged: Bool = true
    @AppStorage("importStoragePinned")
    private var storagePinned: Bool = true

    @State
    private var documentContent: (name: String, text: String)?
    @Environment(\.openSettings)
    private var openSettings
    @Environment(UiStore.self)
    private var uiStore

    /// Seeds the initially-selected candidate so a preview can render the
    /// populated view. Production constructs `FolderImportTab()`, leaving the
    /// list unselected.
    init(initialSelection: String? = nil) {
        _selectedKey = State(initialValue: initialSelection)
        _documentContent = State(initialValue: nil)
    }

    private var selectedCandidate: Candidate? {
        guard let key = selectedKey else {
            return nil
        }
        return importStore.folderCandidates[key]
    }

    var body: some View {
        ZStack {
            if importStore.watchedFolders.isEmpty {
                emptyState
            }
            else {
                HSplitView {
                    candidateList
                        .frame(minWidth: 200, idealWidth: 250, maxWidth: 350)
                    if let candidate = selectedCandidate {
                        mainPane(for: candidate)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                    else {
                        ContentUnavailableView(
                            "Select a folder",
                            systemImage: "folder",
                            description: Text(
                                "Choose a scanned folder to search for metadata"
                            ),
                        )
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }

            // Document viewer overlay
            if let doc = documentContent {
                Color.black.opacity(0.5)
                    .ignoresSafeArea()
                    .onTapGesture { documentContent = nil }
                DocumentViewerView(
                    name: doc.name,
                    text: doc.text,
                    onClose: { documentContent = nil }
                )
                .frame(width: 750, height: 600)
                .background(Theme.surface)
                .clipShape(RoundedRectangle(cornerRadius: 10))
                .shadow(radius: 20)
            }
        }
        .onChange(of: selectedKey) { _, _ in
            uiStore.lightbox = nil
        }
        .task {
            // Hydrate the durable watched-folder list and scan each so its
            // releases stream in as candidates. The reducer keeps already-loaded
            // candidates' in-progress state on re-scan, so this is safe to re-run.
            importStore.watchedFolders = importer.watchedFolders()
            do {
                try importer.scanWatchedFolders()
            }
            catch {
                uiStore.showError(
                    "Scan failed: \(error.localizedDescription)"
                )
            }
        }
    }

    // MARK: - Empty state

    private var emptyState: some View {
        VStack(spacing: 12) {
            Button(action: { pickFolderAndAdd() }) {
                Image(systemName: "plus.circle")
                    .font(.system(size: 48, weight: .thin))
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            Text("Add a folder to import music from")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// Pick a folder and add it to the watch list; its releases scan in as
    /// candidates and the folder persists across restarts.
    private func pickFolderAndAdd() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.message = "Select a folder to watch for music to import"
        panel.prompt = "Add"
        guard panel.runModal() == .OK, let url = panel.url else {
            return
        }
        do {
            try importer.addWatchedFolder(url.path)
        }
        catch {
            uiStore.showError(
                "Couldn't add folder: \(error.localizedDescription)"
            )
        }
    }

    // MARK: - Candidate list

    private var candidateSelectionBinding: Binding<String?> {
        Binding(
            get: { selectedKey },
            set: { key in
                guard let key,
                    let candidate = importStore.folderCandidates[key]
                else {
                    return
                }
                selectCandidate(candidate)
            },
        )
    }

    private func selectCandidate(_ candidate: Candidate) {
        guard case .folder(let folderPath, _) = candidate.source else {
            return
        }

        selectedKey = candidate.key

        // Identify gate: only kick off on the first selection. Subsequent
        // re-selects (including back-to-identify from Confirming) keep the
        // last state. Identify also starts extraction, which streams the
        // candidate's signals (disc ID, barcodes, classified text).
        if case .idle = candidate.identifyState {
            importer.autoIdentifyFolder(folderPath, folderPath)
        }
    }

    // MARK: - Candidate list UI

    private var candidateList: some View {
        ImportCandidateListContent(
            importStore: importStore,
            selectedKey: candidateSelectionBinding,
            isLikelyDupe: { name in
                do {
                    return try importer.isSourceFolderNameImported(name)
                }
                catch {
                    logger.warning(
                        "Failed to check if folder is imported: \(error)"
                    )
                    return false
                }
            },
            onAddFolder: { pickFolderAndAdd() },
            onRemoveFolder: { path in removeWatchedFolder(path) },
            onSkip: { key, skipped in setCandidateSkipped(key, skipped) },
        )
    }

    /// Mark the candidate at `key` skipped or unskipped. The reducer re-tabs the
    /// row when the `candidateSkipChanged` event arrives.
    private func setCandidateSkipped(_ key: String, _ skipped: Bool) {
        do {
            try importer.setCandidateSkipped(key, skipped)
        }
        catch {
            uiStore.showError(
                "Couldn't update skip state: \(error.localizedDescription)"
            )
        }
    }

    /// Stop watching `path`. If the selected candidate lived in that folder,
    /// clear the selection — the reducer drops the folder's candidates when the
    /// new watched-folder list arrives.
    private func removeWatchedFolder(_ path: String) {
        if let key = selectedKey,
            importStore.folderCandidates[key]?.watchedFolderPath == path
        {
            selectedKey = nil
        }
        do {
            try importer.removeWatchedFolder(path)
        }
        catch {
            uiStore.showError(
                "Couldn't remove folder: \(error.localizedDescription)"
            )
        }
    }

    // MARK: - Main pane

    private func mainPane(for candidate: Candidate) -> some View {
        ImportMainPane(
            files: candidate.files,
            onOpenGallery: { index in
                let files = candidate.files
                guard files.artwork.indices.contains(index) else {
                    return
                }
                let tappedPath = files.artwork[index].localPath
                let items = files.artwork.map { file in
                    LightboxItem(
                        id: file.localPath,
                        label: file.name,
                        path: file.localPath
                    )
                }
                uiStore.presentLightbox(items: items, preferring: tappedPath)
            },
            onOpenDocument: { name, text in
                documentContent = (name: name, text: text)
            },
            onPreviewAudio: { path in
                previewAudio.previewPlay(path)
            },
            onError: { uiStore.showError($0) },
            previewState: importStore.previewState,
        ) {
            resultPane(for: candidate)
        }
        .animation(nil, value: selectedKey)
    }

    /// True while the confirm pane is docked — during detail load and while
    /// confirming.
    private func paneOpen(_ candidate: Candidate) -> Bool {
        candidate.mode == .loadingDetail || candidate.mode == .confirming
    }

    /// Search/results above, the confirm pane docked at the bottom. The results
    /// stay visible and scrollable; the pane slides up when a pressing is
    /// picked and is drag-resizable.
    private func resultPane(for candidate: Candidate) -> some View {
        let open = paneOpen(candidate)
        return ImportResultPane(
            open: open,
            onClose: { closePane(candidate) },
            top: {
                searchAndResultsPane(
                    for: candidate,
                    selectedReleaseId: open
                        ? candidate.identityChoice?.releaseRef?.releaseId : nil
                )
            },
            pane: { paneContent(for: candidate) }
        )
    }

    /// Pane body: a loading spinner while the source detail loads, the
    /// editable confirm form once it's in, and nothing when the pane is closed
    /// (it's clipped to zero height then).
    @ViewBuilder
    private func paneContent(for candidate: Candidate) -> some View {
        switch candidate.mode {
        case .loadingDetail:
            ProgressView("Loading release details...")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .confirming:
            confirmationView(for: candidate)
        case .identifying:
            Color.clear
        }
    }

    /// Close the pane and drop the seed cluster so reopening the identify
    /// surface leaves no stale identity claim or file-tag edit behind.
    private func closePane(_ candidate: Candidate) {
        importStore.mutateCandidate(forKey: candidate.key) { c in
            c.mode = .identifying
            c.releaseDetailBridge = nil
            c.identityChoice = nil
            c.editValues = nil
        }
    }

    private func searchAndResultsPane(
        for candidate: Candidate,
        selectedReleaseId: String?
    ) -> some View {
        ImportSearchFlow.buildSearchPane(
            importer: importer,
            library: library,
            importStore: importStore,
            configStore: configStore,
            key: candidate.key,
            candidate: candidate,
            localTrackCount: candidate.trackCount,
            openSettings: { openSettings() },
            selectedReleaseId: selectedReleaseId,
            onAddAsUnknown: {
                guard case .folder(let folderPath, _) = candidate.source else {
                    return
                }
                ImportSearchFlow.addAsUnknown(
                    importer: importer,
                    importStore: importStore,
                    key: candidate.key,
                    folderPath: folderPath
                )
            },
        )
    }

    private func confirmationView(for candidate: Candidate) -> some View {
        let key = candidate.key
        let detail = candidate.releaseDetail
        // For Unknown imports there's no source release detail —
        // remote cover art and library status are absent, the track
        // count comes from the editor (one entry per audio file), and
        // there's nothing to mismatch against.
        let remoteCoverArts = detail?.coverArt ?? []
        let hasCoverOptions =
            !remoteCoverArts.isEmpty
            || (candidate.files.artwork.isEmpty == false)
        let libraryStatus =
            detail.flatMap { candidate.libraryStatuses[$0.releaseId] }
        let trackCountMismatch = detail?.trackCountMismatch ?? false
        let expectedTrackCount: UInt32 = {
            if let detailCount = detail?.trackCount {
                return detailCount
            }
            if let editTracks = candidate.editValues?.tracks {
                return UInt32(editTracks.count)
            }
            return 0
        }()
        return ImportSearchFlow.buildConfirmationView(
            importStore: importStore,
            key: key,
            trackCountMismatch: trackCountMismatch,
            expectedTrackCount: expectedTrackCount,
            libraryStatus: libraryStatus,
            remoteCoverArts: remoteCoverArts,
            hasCoverOptions: hasCoverOptions,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned,
            importDisabled: false,
            localArtwork: candidate.files.artwork,
            uiStore: uiStore,
            onConfirmImport: {
                commitConfirmedImport(candidate: candidate)
            },
            onViewInLibrary: { albumId in
                uiStore.navigateToAlbum(albumId)
            },
            coverContent: {
                folderCoverThumb(
                    source: candidate.selectedCover.map {
                        ImageLoader.Source(bridge: $0.thumbnailSource)
                    },
                )
            },
            actionExtra: EmptyView.init,
        )
    }

    // MARK: - Folder-specific cover thumbnail (supports local artwork)

    private func folderCoverThumb(source: ImageLoader.Source?)
        -> some View
    {
        Group {
            if let source {
                ImageView(
                    source: source,
                    pointSize: 80
                )
            }
            else {
                Theme.placeholder
            }
        }
        .frame(width: 80, height: 80)
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }

    private func commitConfirmedImport(candidate: Candidate) {
        guard case .folder(let folderPath, _) = candidate.source else {
            return
        }
        // Start each attempt from a clean error state so a prior failed
        // commit's banner doesn't linger over a now-succeeding retry.
        importStore.mutateCandidate(forKey: candidate.key) { $0.error = nil }
        let coverSelection = candidate.selectedCover?.selection

        let storageMode = configStore.config.importStorageMode(
            managed: storageManaged,
            pinned: storagePinned
        )

        // The identity choice was picked at row-time (or set to
        // `.unknown` by the "Add as Unknown" link) and stashed on the
        // candidate; the editor overlay is the candidate's current
        // `editValues` (seeded from the detail or file-tag projection,
        // possibly mutated by the user on this confirmation page).
        // Both fields are written before the candidate transitions to
        // `.confirming` mode, which is the only mode that surfaces
        // this commit button — absence here is a structural bug.
        guard let identityChoice = candidate.identityChoice,
            let editValues = candidate.editValues
        else {
            fatalError("commit reached without identity choice or edit values")
        }

        commitImport(
            store: importStore,
            key: candidate.key,
            rawEdit: editValues
        ) {
            try importer.startImport(
                candidate.key,
                folderPath,
                coverSelection,
                storageMode,
                identityChoice.bridge,
                $0
            )
        }
    }
}

#if DEBUG
    #Preview("Folder import — whole view") {
        FolderImportTab(
            initialSelection: PreviewData.folderCandidates.first?.key
        )
        .frame(width: 1100, height: 700)
        .environment(MediaPaths.stub)
        .environment(UiStore())
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(PreviewData.configStore)
        .environment(Library.stub)
        .environment(PreviewAudio.stub)
        .environment(PreviewData.folderImportStore)
        .environment(
            // FolderImportTab's .task re-hydrates watchedFolders from the
            // Importer, so it must return the seeded folder or the view falls
            // back to its empty state.
            Importer(watchedFolders: { [PreviewData.importWatchedFolder] })
        )
    }
#endif
