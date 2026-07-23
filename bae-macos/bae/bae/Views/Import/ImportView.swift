import AppKit
import BaeKit
import SwiftUI

struct ImportView: View {
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

    // Last-used storage choices, persisted; only consulted when a cloud home
    // exists (toggles hidden otherwise). Managed and pinned are orthogonal:
    // `managed` picks the storage state, `pinned` is passed separately to
    // `startImport`. Config.importStorageMode forces Unmanaged without a home.
    @AppStorage("importStorageManaged")
    var storageManaged: Bool = true
    @AppStorage("importStoragePinned")
    var storagePinned: Bool = true

    @State
    var documentContent: (name: String, text: String)?
    @Environment(\.openSettings)
    var openSettings
    @Environment(UiStore.self)
    var uiStore

    var selectedCandidate: Candidate? {
        guard let key = uiStore.selectedFolderCandidate else {
            return nil
        }
        return importStore.folderCandidates[key]
    }

    var body: some View {
        VStack(spacing: 0) {
            Divider()
            ZStack {
                if importStore.watchedFolders.isEmpty {
                    emptyState
                }
                else {
                    splitContent
                }

                documentOverlay
            }
            .onChange(of: uiStore.selectedFolderCandidate) { _, _ in
                uiStore.lightbox = nil
            }
            .task(id: sourceFolderNames) {
                await importStore.refreshImportedSourceFolderNames(
                    sourceFolderNames,
                    isImported: importer.isSourceFolderNameImported,
                    onError: { uiStore.showError($0) }
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
    func pickFolderAndAdd() {
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
                        "Couldn't add folder: \(error.displayLine)"
                )
            )
        }
    }

    // MARK: - Candidate selection

    var candidateSelectionBinding: Binding<String?> {
        Binding(
            get: { uiStore.selectedFolderCandidate },
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

        uiStore.selectFolderCandidate(candidate.key)

        // Identify gate: only kick off on the first selection. Subsequent
        // re-selects (including back-to-identify from Confirming) keep the
        // last state. Identify also starts extraction, which streams the
        // candidate's signals (disc ID, barcodes, classified text).
        if case .idle = candidate.identifyState {
            importer.autoIdentifyFolder(folderPath, folderPath)
        }
    }

    private var sourceFolderNames: [String] {
        Array(Set(importStore.folderCandidates.values.map(\.displayName)))
            .sorted()
    }

    /// Mark the candidate at `key` skipped or unskipped. The import-candidate
    /// projection re-tabs the row once the skip toggle round-trips through
    /// core.
    func setCandidateSkipped(_ key: String, _ skipped: Bool) {
        do {
            try importer.setCandidateSkipped(key, skipped)
        }
        catch {
            uiStore.showError(
                String(
                    localized:
                        "Couldn't update skip state: \(error.displayLine)"
                )
            )
        }
    }

    /// Stop watching `path`. If the selected candidate lived in that folder,
    /// clear the selection — the import-candidate projection drops the
    /// folder's candidates when the new watched-folder list arrives.
    func removeWatchedFolder(_ path: String) {
        if let key = uiStore.selectedFolderCandidate,
            importStore.folderCandidates[key]?.watchedFolderPath == path
        {
            uiStore.selectFolderCandidate(nil)
        }
        do {
            try importer.removeWatchedFolder(path)
        }
        catch {
            uiStore.showError(
                String(
                    localized:
                        "Couldn't remove folder: \(error.displayLine)"
                )
            )
        }
    }
}

#if DEBUG
    #Preview("Import — whole view") {
        // Seeds the initially-selected candidate so the preview renders the
        // populated view; production starts with nothing selected.
        let uiStore = UiStore()
        uiStore.selectFolderCandidate(PreviewData.folderCandidates.first?.key)
        return ImportView()
            .frame(width: 1400, height: 800)
            .environment(uiStore)
            .importPreviewEnvironment()
            .environment(Library.stub)
            .environment(PreviewAudio.stub)
            .environment(PreviewData.folderImportStore)
            .environment(Importer())
    }
#endif
