import BaeKit
import SwiftUI

/// In-memory disclosure choices for candidate metadata forms. A candidate
/// without a choice uses the draft shape as its initial state: blank unmatched
/// drafts open for entry, while populated or matched drafts stay folded.
struct CandidateMetadataDetailsState: Equatable {
    private var expandedByCandidate: [String: Bool] = [:]

    mutating func establishInitialState(
        for key: String,
        draftIsBlank: Bool,
        hasMatchedRelease: Bool
    ) {
        guard expandedByCandidate[key] == nil else { return }
        expandedByCandidate[key] = draftIsBlank && !hasMatchedRelease
    }

    func isExpanded(
        for key: String,
        draftIsBlank: Bool,
        hasMatchedRelease: Bool
    ) -> Bool {
        expandedByCandidate[key] ?? (draftIsBlank && !hasMatchedRelease)
    }

    mutating func setExpanded(_ expanded: Bool, for key: String) {
        expandedByCandidate[key] = expanded
    }

    mutating func externalReleaseApplied(for key: String) {
        expandedByCandidate[key] = false
    }
}

struct ImportView: View {
    /// End the window's active field edit before a metadata source replaces
    /// the candidate draft.
    let endEditing: () -> Void

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
    @AppStorage(StoragePinPreference.userDefaultsKey)
    var storagePinned: Bool = true

    @State
    var documentContent: (name: String, text: String)?
    /// Event-driven candidate writes keyed by candidate, so a repeated command
    /// cancels the operation it replaces and leaving the import view cancels
    /// every command the view started.
    @State
    var candidateMutationTasks: [String: Task<Void, Never>] = [:]
    /// Coordinates active field writes with metadata replacement. A source
    /// application must wait for the draft the person was editing to commit.
    @State
    var editingCommands = EditingCommitCommands()
    /// The metadata form's disclosure choice belongs to its candidate and
    /// survives selecting another row and returning during this import view.
    @State
    var metadataDetailsState = CandidateMetadataDetailsState()
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
    @Environment(ImportListSlot.self)
    var listSlot

    var selectedCandidate: Candidate? {
        guard uiStore.selectedFolderCandidates.count == 1,
            let key = uiStore.selectedFolderCandidates.first
        else {
            return nil
        }
        return importStore.selectedCandidates[key]
    }

    func commitAndEndEditing() async {
        await editingCommands.commitActiveEdits()
        endEditing()
    }

    func metadataDetailsExpanded(for candidate: Candidate) -> Binding<Bool> {
        Binding(
            get: {
                metadataDetailsState.isExpanded(
                    for: candidate.key,
                    draftIsBlank: candidate.metadataDraftIsBlank,
                    hasMatchedRelease: candidate.pickedRelease != nil
                )
            },
            set: { expanded in
                metadataDetailsState.setExpanded(
                    expanded,
                    for: candidate.key
                )
            }
        )
    }

    func establishMetadataDetailsInitialState(for candidate: Candidate) {
        metadataDetailsState.establishInitialState(
            for: candidate.key,
            draftIsBlank: candidate.metadataDraftIsBlank,
            hasMatchedRelease: candidate.pickedRelease != nil
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            Divider()
            ZStack {
                if let failure = listSlot.loadFailure {
                    failedState(failure)
                }
                else if importStore.watchedFolders.isEmpty {
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
            .onDisappear {
                for task in candidateMutationTasks.values {
                    task.cancel()
                }
                candidateMutationTasks.removeAll()
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

    /// The list could not be read at all, so nothing here is known — including
    /// whether any folder is being watched. Shown in place of the empty state,
    /// which would otherwise say the library has no folders when the truth is
    /// that nobody could look.
    private func failedState(_ failure: DisplayError) -> some View {
        VStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 40, weight: .thin))
                .foregroundStyle(.red)
            Text("The import list couldn't be read")
                .font(.callout)
            if let detail = failure.detailSummary {
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .textSelection(.enabled)
            }
            Button("Retry") {
                listSlot.startLoad()
            }
        }
        .padding(40)
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
                // No line means a cancellation, which raises no alert.
                if let line = error.displayLine {
                    uiStore.showError(
                        String(
                            localized:
                                "Couldn't read what \(sheet.file.name) can describe: \(line)"
                        )
                    )
                }
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
                if let line = error.displayLine {
                    uiStore.showError(
                        String(localized: "Couldn't update skip state: \(line)")
                    )
                }
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
                        importStore.selectedCandidates[$0]?.watchedFolderPath
                            == path
                    }
                )
                uiStore.removeFolderCandidateSelection(removed)
            }
            catch {
                if let line = error.displayLine {
                    uiStore.showError(
                        String(localized: "Couldn't remove folder: \(line)")
                    )
                }
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
        @MainActor
        fileprivate func importTabPreviewEnvironment(
            scene: ImportPreviewFixture,
            uiStore: UiStore
        )
            -> some View
        {
            self
                .environment(scene.store)
                .environment(scene.slot(uiStore: uiStore))
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
        let scene = PreviewData.importSmokeTestScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, uiStore: uiStore)
    }

    #Preview("Import tab — a release settled") {
        let uiStore = ImportTabPreview.uiStore(
            tab: .pending,
            selected: PreviewData.importTabCandidate.key,
            ticked: [PreviewData.importTabCandidate.key]
        )
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, uiStore: uiStore)
    }

    #Preview("Import tab — multiple pressings") {
        let uiStore = ImportTabPreview.uiStore(
            tab: .pending,
            selected: PreviewData.importTabSeveralMatchesCandidate.key
        )
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, uiStore: uiStore)
    }

    #Preview("Import tab — identity signals disagree") {
        let uiStore = ImportTabPreview.uiStore(
            tab: .pending,
            selected: PreviewData.importTabDisagreementCandidate.key
        )
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, uiStore: uiStore)
    }

    #Preview("Import tab — track counts disagree") {
        let uiStore = ImportTabPreview.uiStore(
            tab: .pending,
            selected: PreviewData.importTabTrackMismatchCandidate.key
        )
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, uiStore: uiStore)
    }

    #Preview("Import tab — release already in library") {
        let uiStore = ImportTabPreview.uiStore(
            tab: .pending,
            selected: PreviewData.importTabAlreadyInLibraryCandidate.key
        )
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, uiStore: uiStore)
    }

    #Preview("Import tab — no release matched") {
        let uiStore = ImportTabPreview.uiStore(
            tab: .pending,
            selected: PreviewData.importTabNoMatchCandidate.key
        )
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, uiStore: uiStore)
    }

    #Preview("Import tab — completed imports") {
        let uiStore = ImportTabPreview.uiStore(tab: .done)
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, uiStore: uiStore)
    }

    #Preview("Import tab — skipped and invalid folders") {
        let uiStore = ImportTabPreview.uiStore(tab: .skipped)
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, uiStore: uiStore)
    }
#endif
