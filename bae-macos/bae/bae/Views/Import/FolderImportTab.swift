import AppKit
import BaeKit
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
    // Last-used storage choices, persisted; only consulted when a cloud home
    // exists (toggles hidden otherwise). Managed and pinned are orthogonal:
    // `managed` picks the storage state, `pinned` is passed separately to
    // `startImport`. Config.importStorageMode forces Unmanaged without a home.
    @AppStorage("importStorageManaged")
    private var storageManaged: Bool = true
    @AppStorage("importStoragePinned")
    private var storagePinned: Bool = true

    @State
    private var documentContent: (name: String, text: String)?
    @State
    private var importedSourceFolderNames: Set<String> = []
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
                splitContent
            }

            documentOverlay
        }
        .onChange(of: selectedKey) { _, _ in
            uiStore.lightbox = nil
        }
        .task {
            // Scan watched folders; candidate and folder state refreshes through
            // invalidation projections.
            do {
                try importer.scanWatchedFolders()
            }
            catch {
                uiStore.showError(
                    String(
                        localized: "Scan failed: \(error.localizedDescription)"
                    )
                )
            }
        }
        .task(id: sourceFolderNames) {
            await refreshImportedSourceFolderNames(sourceFolderNames)
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
        panel.message = String(
            localized: "Select a folder to watch for music to import"
        )
        panel.prompt = String(localized: "Add")
        guard panel.runModal() == .OK, let url = panel.url else {
            return
        }
        do {
            try importer.addWatchedFolder(url.path)
        }
        catch {
            uiStore.showError(
                String(
                    localized:
                        "Couldn't add folder: \(error.localizedDescription)"
                )
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

    private var sourceFolderNames: [String] {
        Array(Set(importStore.folderCandidates.values.map(\.displayName)))
            .sorted()
    }

    private func refreshImportedSourceFolderNames(_ names: [String]) async {
        var imported: Set<String> = []
        for name in names {
            do {
                if try await importer.isSourceFolderNameImported(name) {
                    imported.insert(name)
                }
            }
            catch {
                logger.warning(
                    "Failed to check if folder is imported: \(error)"
                )
                uiStore.showError(error.localizedDescription)
                return
            }
        }
        importedSourceFolderNames = imported
    }

    /// Mark the candidate at `key` skipped or unskipped. The import-candidate
    /// projection re-tabs the row once the skip toggle round-trips through
    /// core.
    private func setCandidateSkipped(_ key: String, _ skipped: Bool) {
        do {
            try importer.setCandidateSkipped(key, skipped)
        }
        catch {
            uiStore.showError(
                String(
                    localized:
                        "Couldn't update skip state: \(error.localizedDescription)"
                )
            )
        }
    }

    /// Stop watching `path`. If the selected candidate lived in that folder,
    /// clear the selection — the import-candidate projection drops the
    /// folder's candidates when the new watched-folder list arrives.
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
                String(
                    localized:
                        "Couldn't remove folder: \(error.localizedDescription)"
                )
            )
        }
    }
}

// MARK: - Candidate list

extension FolderImportTab {
    fileprivate var candidateList: some View {
        ImportCandidateListContent(
            importStore: importStore,
            selectedKey: candidateSelectionBinding,
            isLikelyDupe: importedSourceFolderNames.contains,
            onAddFolder: { pickFolderAndAdd() },
            onRemoveFolder: { path in removeWatchedFolder(path) },
            onSkip: { key, skipped in setCandidateSkipped(key, skipped) },
        )
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
                        source: .local(path: file.localPath)
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

}

// MARK: - Search, results, and confirm

extension FolderImportTab {
    fileprivate func searchAndResultsPane(
        for candidate: Candidate,
        selectedReleaseId: String?
    ) -> some View {
        ImportSearchFlow.buildSearchPane(
            services: ImportSearchFlow.ImportServices(
                importer: importer,
                library: library,
                importStore: importStore,
                configStore: configStore
            ),
            input: ImportSearchFlow.SearchPaneInput(
                candidate: candidate,
                key: candidate.key,
                localTrackCount: candidate.trackCount,
                selectedReleaseId: selectedReleaseId
            ),
            openSettings: { openSettings() },
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

    fileprivate func confirmationView(for candidate: Candidate) -> some View {
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
            inputs: ImportSearchFlow.ConfirmationInputs(
                importStore: importStore,
                key: key,
                uiStore: uiStore,
                trackCountMismatch: trackCountMismatch,
                expectedTrackCount: expectedTrackCount,
                libraryStatus: libraryStatus,
                remoteCoverArts: remoteCoverArts,
                hasCoverOptions: hasCoverOptions,
                storageManaged: $storageManaged,
                storagePinned: $storagePinned,
                importDisabled: false,
                localArtwork: candidate.files.artwork
            ),
            callbacks: ImportSearchFlow.ConfirmationCallbacks(
                onConfirmImport: {
                    commitConfirmedImport(candidate: candidate)
                },
                onViewInLibrary: { albumId in
                    uiStore.navigateToAlbum(albumId)
                }
            ),
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

    fileprivate func folderCoverThumb(source: ImageLoader.Source?)
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

    fileprivate func commitConfirmedImport(candidate: Candidate) {
        guard case .folder(let folderPath, _) = candidate.source else {
            return
        }
        // Start each attempt from a clean error state so a prior failed
        // commit's banner doesn't linger over a now-succeeding retry.
        importStore.mutateCandidate(forKey: candidate.key) { $0.error = nil }
        let coverSelection = candidate.selectedCover?.selection

        let storageMode = configStore.config.importStorageMode(
            managed: storageManaged
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
                storagePinned,
                identityChoice.bridge,
                $0
            )
        }
    }
}

// MARK: - Body layout

extension FolderImportTab {
    @ViewBuilder
    fileprivate var splitContent: some View {
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

    @ViewBuilder
    fileprivate var documentOverlay: some View {
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
}

#if DEBUG
    #Preview("Folder import — whole view") {
        FolderImportTab(
            initialSelection: PreviewData.folderCandidates.first?.key
        )
        .frame(width: 1400, height: 800)
        .importPreviewEnvironment()
        .environment(Library.stub)
        .environment(PreviewAudio.stub)
        .environment(PreviewData.folderImportStore)
        .environment(Importer())
    }
#endif
