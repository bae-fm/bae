import BaeKit
import SwiftUI

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
    @Environment(Playback.self)
    var playback
    @Environment(ReleaseEditor.self)
    var releaseEditor
    @Environment(ImportStore.self)
    var importStore
    @Environment(ConfigStore.self)
    var configStore

    /// The stored import storage choice; shown only when a cloud home exists.
    var storageCloud: Binding<Bool> {
        Binding(
            get: { configStore.config.importStorage.cloud },
            set: { enabled in
                writeStorageChoice {
                    try await importer.setImportToCloud(enabled)
                }
            }
        )
    }

    var storagePinned: Binding<Bool> {
        Binding(
            get: { configStore.config.importStorage.pinned },
            set: { enabled in
                writeStorageChoice {
                    try await importer.setImportPinned(enabled)
                }
            }
        )
    }

    private func writeStorageChoice(
        _ write: @escaping @MainActor () async throws -> Void
    ) {
        Task { @MainActor in
            do { try await write() }
            catch { uiStore.showError(error) }
        }
    }

    /// Candidate writes by candidate key; a repeated command cancels the one
    /// it replaces, and leaving the view cancels them all.
    @State
    var candidateMutationTasks: [String: Task<Void, Never>] = [:]
    /// Commits the active field edit before a metadata source replaces the
    /// draft.
    @State
    var editingCommands = EditingCommitCommands()
    /// An action awaiting confirmation because it replaces the person's
    /// choices.
    @State
    var candidateActionConfirmation: ImportCandidateActionOffer?
    @Environment(\.openSettings)
    var openSettings
    @Environment(UiStore.self)
    var uiStore
    @Environment(ImportListSlot.self)
    var listSlot
    @Environment(ImportSelection.self)
    var importSelection

    var selectedCandidate: Candidate? {
        importSelection.summary.single.flatMap {
            importStore.selectedCandidates[$0]
        }
    }

    func commitAndEndEditing() async {
        await editingCommands.commitActiveEdits()
        endEditing()
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
            }
            .onChange(of: importSelection.summary.single) { _, _ in
                uiStore.lightbox = nil
            }
            .onDisappear {
                for task in candidateMutationTasks.values {
                    task.cancel()
                }
                candidateMutationTasks.removeAll()
            }
        }
        .alert(
            "Replace selected metadata?",
            isPresented: Binding(
                get: { candidateActionConfirmation != nil },
                set: { if !$0 { candidateActionConfirmation = nil } }
            ),
            presenting: candidateActionConfirmation
        ) { offer in
            Button(
                offer.action.label(count: offer.applicable),
                role: .destructive
            ) { performCandidateAction(offer) }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            Text(
                "This replaces metadata and cover choices for the selected folders. Source files and track layout are unchanged."
            )
        }
    }

    // MARK: - Empty state

    private var emptyState: some View {
        VStack(spacing: ThemeSpace.group) {
            Button(action: {
                uiStore.setImportFolderPickerPresented(true)
            }) {
                Image(systemName: "plus.circle")
                    .themeIcon(.hero)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            Text("Add a folder to import music from")
                .themeText(.body)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// Shown when the list could not be read, instead of an empty state that
    /// would wrongly say no folder is watched.
    private func failedState(_ failure: DisplayError) -> some View {
        VStack(spacing: ThemeSpace.group) {
            Image(systemName: "exclamationmark.triangle.fill")
                .themeIcon(.hero)
                .foregroundStyle(Theme.danger)
            Text("The import list couldn't be read")
                .themeText(.body)
            if let detail = failure.detailSummary {
                Text(detail)
                    .themeText(.mono)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .textSelection(.enabled)
            }
            Button("Retry") {
                listSlot.startLoad()
            }
        }
        .padding(ThemeSpace.page)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// Stop watching `path`. Its candidates leave the selection in the same
    /// write that removes them.
    func removeWatchedFolder(_ path: String) {
        Task {
            do {
                try await importer.removeWatchedFolder(path)
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
}

#if DEBUG
    /// The whole Import tab from a seeded environment; its stores go on
    /// inside `importPreviewEnvironment` so they win over the ones it installs.
    @MainActor
    private struct ImportTabPreview {
        let uiStore = UiStore()
        let selection = ImportSelection()

        /// The sidebar tab and selected candidate for this preview.
        init(tab: BridgeTriageTab, selected: String? = nil) {
            uiStore.setImportCandidateTab(tab)
            selection.apply(
                BridgeSelectionSummary(
                    count: selected == nil ? 0 : 1,
                    single: selected,
                    offers: []
                )
            )
        }
    }

    extension View {
        @MainActor
        fileprivate func importTabPreviewEnvironment(
            scene: ImportPreviewFixture,
            preview: ImportTabPreview
        )
            -> some View
        {
            self
                .environment(scene.store)
                .environment(scene.slot(uiStore: preview.uiStore))
                .environment(preview.uiStore)
                .environment(preview.selection)
                .importPreviewEnvironment()
                .environment(Library.stub())
                .environment(PreviewAudio.stub())
                .environment(PreviewData.importTabImporter())
                .frame(width: 1440, height: 900)
                .preferredColorScheme(.dark)
        }
    }

    #Preview("Import tab — smoke test") {
        let preview = ImportTabPreview(
            tab: .pending,
            selected: PreviewData.importTabCandidate.key
        )
        let scene = PreviewData.importSmokeTestScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, preview: preview)
    }

    #Preview("Import tab — a release settled") {
        let preview = ImportTabPreview(
            tab: .pending,
            selected: PreviewData.importTabCandidate.key
        )
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, preview: preview)
    }

    #Preview("Import tab — multiple pressings") {
        let preview = ImportTabPreview(
            tab: .pending,
            selected: PreviewData.importTabSeveralMatchesCandidate.key
        )
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, preview: preview)
    }

    #Preview("Import tab — identity signals disagree") {
        let preview = ImportTabPreview(
            tab: .pending,
            selected: PreviewData.importTabDisagreementCandidate.key
        )
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, preview: preview)
    }

    #Preview("Import tab — the release lists no tracks") {
        let preview = ImportTabPreview(
            tab: .pending,
            selected: PreviewData.importTabNoTracklistCandidate.key
        )
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, preview: preview)
    }

    #Preview("Import tab — release already in library") {
        let preview = ImportTabPreview(
            tab: .pending,
            selected: PreviewData.importTabAlreadyInLibraryCandidate.key
        )
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, preview: preview)
    }

    #Preview("Import tab — no release matched") {
        let preview = ImportTabPreview(
            tab: .pending,
            selected: PreviewData.importTabNoMatchCandidate.key
        )
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, preview: preview)
    }

    #Preview("Import tab — completed imports") {
        let preview = ImportTabPreview(tab: .done)
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, preview: preview)
    }

    #Preview("Import tab — skipped and invalid folders") {
        let preview = ImportTabPreview(tab: .skipped)
        let scene = PreviewData.importTabScene()
        ImportView(endEditing: {})
            .importTabPreviewEnvironment(scene: scene, preview: preview)
    }
#endif
