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

    /// All folder candidates in the store. One candidate per release.
    private var folderCandidates: [Candidate] {
        Array(importStore.folderCandidates.values)
    }

    private var selectedCandidate: Candidate? {
        guard let key = selectedKey else {
            return nil
        }
        return importStore.folderCandidates[key]
    }

    var body: some View {
        ZStack {
            if folderCandidates.isEmpty {
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
    }

    // MARK: - Empty state

    private var emptyState: some View {
        VStack(spacing: 12) {
            Button(action: { pickFolderAndScan(clearExisting: true) }) {
                Image(systemName: "plus.circle")
                    .font(.system(size: 48, weight: .thin))
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            Text("Scan a folder to import music")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// Pick a folder and scan it. `clearExisting` true replaces the current
    /// candidate list (the empty-state action); false appends to it (the
    /// "add another" action).
    private func pickFolderAndScan(clearExisting: Bool) {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.message = "Select a folder containing music to import"
        panel.prompt = "Scan"
        guard panel.runModal() == .OK, let url = panel.url else {
            return
        }
        do {
            try importer.enqueueFolderScan(url.path, clearExisting)
        }
        catch {
            uiStore.showError("Scan failed: \(error.localizedDescription)")
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
        guard case .folder(let folderPath) = candidate.source else {
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
            candidates: folderCandidates,
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
            onAdd: { pickFolderAndScan(clearExisting: false) },
            onClearAll: {
                importer.clearAllCandidates()
                selectedKey = nil
            },
            onClearCompleted: {
                let completedKeys =
                    folderCandidates
                    .filter { candidate in
                        if case .complete = candidate.importStatus {
                            return true
                        }
                        return false
                    }
                    .map(\.key)
                let completedSet = Set(completedKeys)
                for key in completedSet {
                    importer.removeCandidate(key)
                }
                if let key = selectedKey, completedSet.contains(key) {
                    selectedKey = nil
                }
            },
            onRemove: { key in
                if selectedKey == key {
                    let all = folderCandidates
                    if let index = all.firstIndex(where: { $0.key == key }) {
                        let neighbor =
                            all.indices.contains(index - 1)
                            ? all[index - 1]
                            : all.indices.contains(index + 1)
                                ? all[index + 1] : nil
                        if let neighbor {
                            selectCandidate(neighbor)
                        }
                        else {
                            selectedKey = nil
                        }
                    }
                }
                importer.removeCandidate(key)
            },
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
            if candidate.mode == .loadingDetail {
                ProgressView("Loading release details...")
            }
            else if candidate.mode == .confirming {
                confirmationView(for: candidate)
            }
            else {
                searchAndResultsPane(for: candidate)
            }
        }
        .animation(nil, value: selectedKey)
    }

    private func searchAndResultsPane(for candidate: Candidate) -> some View {
        ImportSearchFlow.buildSearchPane(
            importer: importer,
            library: library,
            importStore: importStore,
            configStore: configStore,
            key: candidate.key,
            candidate: candidate,
            localTrackCount: candidate.trackCount,
            openSettings: { openSettings() },
            onAddAsUnknown: {
                guard case .folder(let folderPath) = candidate.source else {
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
            onBack: {
                importStore.mutateCandidate(forKey: key) { c in
                    c.mode = .identifying
                    c.releaseDetail = nil
                    // Drop the whole seed cluster so returning to the
                    // identify pane leaves no stale identity claim or
                    // file-tag edit behind.
                    c.identityChoice = nil
                    c.editValues = nil
                }
            },
            onConfirmImport: {
                commitConfirmedImport(candidate: candidate)
            },
            onViewInLibrary: { albumId in
                uiStore.navigateToAlbum(albumId)
            },
            coverContent: {
                folderCoverThumb(
                    selectedUrl: candidate.selectedCoverUrl,
                    artwork: candidate.files.artwork,
                )
            },
            actionExtra: EmptyView.init,
        )
    }

    // MARK: - Folder-specific cover thumbnail (supports local artwork)

    private func folderCoverThumb(selectedUrl: String?, artwork: [FileInfo])
        -> some View
    {
        Group {
            if let url = selectedUrl {
                if url.hasPrefix("local:") {
                    let filename = String(url.dropFirst("local:".count))
                    let localPath =
                        artwork.first(where: { $0.name == filename })?
                        .localPath
                    if let localPath {
                        ImageView(
                            source: .local(path: localPath),
                            pointSize: 80
                        )
                    }
                    else {
                        Theme.placeholder
                    }
                }
                else {
                    ImageView(source: .remote(url: url), pointSize: 80)
                }
            }
            else {
                Theme.placeholder
            }
        }
        .frame(width: 80, height: 80)
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }

    private func commitConfirmedImport(candidate: Candidate) {
        guard case .folder(let folderPath) = candidate.source else {
            return
        }
        // Start each attempt from a clean error state so a prior failed
        // commit's banner doesn't linger over a now-succeeding retry.
        importStore.mutateCandidate(forKey: candidate.key) { $0.error = nil }
        let selectedUrl = candidate.selectedCoverUrl

        let coverSelection: BridgeCoverSelection?
        if let url = selectedUrl {
            if url.hasPrefix("local:") {
                let filename = String(url.dropFirst("local:".count))
                coverSelection = .releaseImage(fileId: filename)
            }
            else {
                // Remote cover URLs only originate from the source
                // detail's `coverArt`, so source is always populated
                // when the user picked a remote URL.
                guard
                    let coverSource = candidate.releaseDetail?.coverArt
                        .first?
                        .source
                else {
                    fatalError(
                        "remote cover selected with no source cover art"
                    )
                }
                coverSelection = .remoteCover(
                    url: url,
                    source: coverSource.bridge
                )
            }
        }
        else {
            coverSelection = nil
        }

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
