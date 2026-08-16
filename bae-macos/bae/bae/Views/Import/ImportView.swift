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
    // exists (toggles hidden otherwise). Cloud and pinned are orthogonal:
    // `cloud` picks the storage state, `pinned` is passed separately to
    // `startImport`. Config.importStorageMode forces Local without a home.
    @AppStorage("importStorageCloud")
    var storageCloud: Bool = true
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
        guard uiStore.selectedFolderCandidates.count == 1,
            let key = uiStore.selectedFolderCandidates.first
        else {
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
            .onChange(of: uiStore.selectedFolderCandidates) { _, _ in
                uiStore.lightbox = nil
            }
            // Fires on both paths to a seedable pane: selecting a row whose
            // identity is already decided — a settled single match, or a
            // choice made before a restart — and the decision landing while
            // its row is selected. `initial` covers a selection that predates
            // this view — the Import tab re-entered with the row selected.
            .onChange(of: pickedResume, initial: true) { _, picked in
                if let picked {
                    applyPickedResume(picked)
                }
            }
        }
    }

    // MARK: - Empty state

    private var emptyState: some View {
        VStack(spacing: 12) {
            Button(action: {
                uiStore.setImportFolderPickerPresented(true)
            }) {
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
                let removed = Set(
                    uiStore.selectedFolderCandidates.filter {
                        importStore.folderCandidates[$0]?.watchedFolderPath
                            == path
                    }
                )
                uiStore.removeFolderCandidateSelection(removed)
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
        uiStore.setFolderCandidateSelection([])
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
    /// The Import tab whole, both halves at once: the triage sidebar it is
    /// steered from and the mapping pane the selected row opens.
    ///
    /// The environment is seeded, never the view — every pixel here comes from
    /// the production `ImportView` and the views under it, so a change to a
    /// row, a card or a column shows up in the canvas without a second
    /// rendering of the same screen to keep in step.
    ///
    /// `importPreviewEnvironment` installs a `UiStore` of its own, so the one
    /// carrying the tab and the selection goes on *before* it: the innermost
    /// value of an environment key is the one the view under it reads.
    private enum ImportTabPreview {
        /// The sidebar's tab, the selected row, and the rows ticked for a bulk
        /// import — the three things that decide what either half of the tab
        /// has to draw. Production opens on Pending with nothing selected and
        /// nothing ticked, which is the one state worth no canvas.
        @MainActor
        static func uiStore(
            tab: BridgeTriageTab,
            selected: String? = nil,
            ticked: [String] = []
        ) -> UiStore {
            let store = UiStore()
            store.setImportCandidateTab(tab)
            store.setFolderCandidateSelection(selected.map { [$0] } ?? [])
            store.selectAllReady(ticked)
            return store
        }
    }

    extension View {
        fileprivate func importTabPreviewEnvironment(
            uiStore: UiStore
        )
            -> some View
        {
            self
                .environment(uiStore)
                .importPreviewEnvironment()
                .environment(Library.stub())
                .environment(PreviewAudio.stub())
                .environment(PreviewData.importTabImporter())
                .frame(width: 1440, height: 900)
                .preferredColorScheme(.dark)
        }
    }

    #Preview("Import tab — smoke test") {
        let uiStore = ImportTabPreview.uiStore(
            tab: .pending,
            selected: PreviewData.importTabCandidate.key,
            ticked: [PreviewData.importTabCandidate.key]
        )
        let importStore: ImportStore = PreviewData.importSmokeTestStore()
        ImportView()
            .environment(importStore)
            .importTabPreviewEnvironment(uiStore: uiStore)
    }

    #Preview("Import tab — a release settled") {
        let uiStore = ImportTabPreview.uiStore(
            tab: .pending,
            selected: PreviewData.importTabCandidate.key,
            ticked: [PreviewData.importTabCandidate.key]
        )
        let importStore: ImportStore = PreviewData.importTabStore()
        ImportView()
            .environment(importStore)
            .importTabPreviewEnvironment(uiStore: uiStore)
    }

    #Preview("Import tab — multiple pressings") {
        let uiStore = ImportTabPreview.uiStore(
            tab: .pending,
            selected: PreviewData.importTabSeveralMatchesCandidate.key
        )
        let importStore: ImportStore = PreviewData.importTabStore()
        ImportView()
            .environment(importStore)
            .importTabPreviewEnvironment(uiStore: uiStore)
    }

    #Preview("Import tab — identity signals disagree") {
        let uiStore = ImportTabPreview.uiStore(
            tab: .pending,
            selected: PreviewData.importTabConflictCandidate.key
        )
        let importStore: ImportStore = PreviewData.importTabStore()
        ImportView()
            .environment(importStore)
            .importTabPreviewEnvironment(uiStore: uiStore)
    }

    #Preview("Import tab — track counts disagree") {
        let uiStore = ImportTabPreview.uiStore(
            tab: .pending,
            selected: PreviewData.importTabTrackMismatchCandidate.key
        )
        let importStore: ImportStore = PreviewData.importTabStore()
        ImportView()
            .environment(importStore)
            .importTabPreviewEnvironment(uiStore: uiStore)
    }

    #Preview("Import tab — release already in library") {
        let uiStore = ImportTabPreview.uiStore(
            tab: .pending,
            selected: PreviewData.importTabAlreadyInLibraryCandidate.key
        )
        let importStore: ImportStore = PreviewData.importTabStore()
        ImportView()
            .environment(importStore)
            .importTabPreviewEnvironment(uiStore: uiStore)
    }

    #Preview("Import tab — no release matched") {
        let uiStore = ImportTabPreview.uiStore(
            tab: .pending,
            selected: PreviewData.importTabNoMatchCandidate.key
        )
        let importStore: ImportStore = PreviewData.importTabStore()
        ImportView()
            .environment(importStore)
            .importTabPreviewEnvironment(uiStore: uiStore)
    }

    #Preview("Import tab — folder boundaries") {
        let uiStore = ImportTabPreview.uiStore(tab: .pending)
        let importStore: ImportStore =
            PreviewData.releaseBoundaryPreviewImportStore
        ImportView()
            .environment(importStore)
            .importTabPreviewEnvironment(uiStore: uiStore)
    }

    #Preview("Import tab — completed imports") {
        let uiStore = ImportTabPreview.uiStore(tab: .done)
        let importStore: ImportStore = PreviewData.importTabStore()
        ImportView()
            .environment(importStore)
            .importTabPreviewEnvironment(uiStore: uiStore)
    }

    #Preview("Import tab — skipped and invalid folders") {
        let uiStore = ImportTabPreview.uiStore(tab: .skipped)
        let importStore: ImportStore = PreviewData.importTabStore()
        ImportView()
            .environment(importStore)
            .importTabPreviewEnvironment(uiStore: uiStore)
    }
#endif
