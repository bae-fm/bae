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
    /// What each of the selected candidate's track sheets may be bound to,
    /// keyed by the sheet's file id. Read from core when the selection's files
    /// change: core probes every audio file to answer, so this is not something
    /// a candidate can carry through the list.
    @State
    var sheetBindingOptions: [String: [BridgeSheetBindingOption]] = [:]
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
            // Fires on both paths to a seedable pane: selecting a Ready row,
            // and the verdict settling to Ready while its row is selected.
            // `initial` covers a selection that predates this view — the
            // Import tab re-entered with a Ready row still selected.
            .onChange(of: readyAutoPick, initial: true) { _, matched in
                if let matched {
                    applyReadyAutoPick(matched)
                }
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
        Task {
            do {
                try await importer.addWatchedFolder(url.path)
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
    }

    /// Read what each of `candidate`'s track sheets may be bound to. One call
    /// per sheet, and each probes the folder's audio, so this runs when the
    /// selection's files change rather than on every render.
    func loadSheetBindingOptions(for candidate: Candidate) async {
        var options: [String: [BridgeSheetBindingOption]] = [:]
        for sheet in candidate.files.trackSheets {
            do {
                options[sheet.file.name] =
                    try await importer.sheetBindingOptions(
                        candidate.key,
                        sheet.file.name
                    )
            }
            catch is CancellationError {
                return
            }
            catch {
                uiStore.showError(
                    String(
                        localized:
                            "Couldn't read what \(sheet.file.name) can describe: \(error.displayLine)"
                    )
                )
            }
        }
        sheetBindingOptions = options
    }

    /// Name the audio `sheetFileId` describes, or clear it with `nil`. Core
    /// persists the decision, drops the candidate's stored identify verdict,
    /// and the candidate invalidation brings the new roles back.
    ///
    /// The slot table is re-read here, because a binding changes what the
    /// folder's audio *is*: one container becomes a dozen tracks, so the
    /// mapping the pane shows has to be recomputed against the release already
    /// picked. The seed it comes back with is for a different set of rows than
    /// the one the user was editing, which is exactly why it replaces them.
    func bindSheet(
        _ candidateKey: String,
        _ sheetFileId: String,
        to audioFileId: String?
    ) async {
        do {
            try await importer.setSheetBinding(
                candidateKey,
                sheetFileId,
                audioFileId
            )
            remapPickedRelease(candidateKey)
        }
        catch is CancellationError {}
        catch {
            uiStore.showError(
                String(
                    localized:
                        "Couldn't change what \(sheetFileId) describes: \(error.displayLine)"
                )
            )
        }
    }

    /// Re-run the prefetch for whatever release is already picked, so the slot
    /// table matches the folder's new shape. A candidate with nothing picked
    /// has no mapping to refresh.
    private func remapPickedRelease(_ candidateKey: String) {
        guard let candidate = importStore.candidate(forKey: candidateKey),
            let releaseId = candidate.pickedReleaseId,
            let detail = candidate.releaseDetailBridge
        else {
            return
        }
        ImportSearchFlow.prefetchAndConfirm(
            library: library,
            importStore: importStore,
            key: candidateKey,
            releaseId: releaseId,
            source: detail.source
        )
    }

    /// Mark the candidate at `key` skipped or unskipped. The import-candidate
    /// projection re-tabs the row once the skip toggle round-trips through
    /// core.
    func setCandidateSkipped(_ key: String, _ skipped: Bool) {
        Task {
            do {
                try await importer.setCandidateSkipped(key, skipped)
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
    }

    /// Stop watching `path`. If the selected candidate lived in that folder,
    /// clear the selection — the import-candidate projection drops the
    /// folder's candidates when the new watched-folder list arrives.
    func removeWatchedFolder(_ path: String) {
        Task {
            do {
                try await importer.removeWatchedFolder(path)
                if let key = uiStore.selectedFolderCandidate,
                    importStore.folderCandidates[key]?.watchedFolderPath == path
                {
                    uiStore.selectFolderCandidate(nil)
                }
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

    func refreshWatchedFolder(_ folder: BridgeWatchedFolder) {
        uiStore.setWatchedFolderRefreshing(folder.path, true)
        Task {
            defer {
                uiStore.setWatchedFolderRefreshing(folder.path, false)
            }
            do {
                try await importer.refreshWatchedFolder(folder.path)
            }
            catch {
                guard let displayed = DisplayError(error) else {
                    return
                }
                uiStore.showError(
                    displayed.addingContext(
                        "\(folder.name) (\(folder.path))"
                    )
                )
            }
        }
    }

    func setFolderReleaseDecision(
        _ key: BridgeFolderReleaseDecisionKey,
        _ decision: BridgeFolderReleaseDecision
    ) {
        uiStore.selectFolderCandidate(nil)
        Task {
            do {
                try await importer.setFolderReleaseDecision(key, decision)
            }
            catch {
                uiStore.showError(error)
            }
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
