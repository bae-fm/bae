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

    /// The stored import destination; shown only when the library syncs.
    var importDestination: Binding<BridgeImportDestination> {
        Binding(
            get: { configStore.config.importDestination },
            set: { destination in
                Task { @MainActor in
                    do { try await importer.setImportDestination(destination) }
                    catch { uiStore.showError(error) }
                }
            }
        )
    }

    /// Candidate writes by what they act on; a repeated command cancels the
    /// one it replaces, and leaving the view cancels them all.
    @State
    var candidateMutationTasks: [ImportTaskKey: Task<Void, Never>] = [:]
    /// Commits the active field edit before a metadata source replaces the
    /// draft.
    @State
    var editingCommands = EditingCommitCommands()
    /// An action awaiting confirmation because it replaces the person's
    /// choices.
    @State
    var candidateActionConfirmation: ImportCandidateActionOffer?
    @Environment(UiStore.self)
    var uiStore
    @Environment(ImportListSlot.self)
    var listSlot
    @Environment(ImportSelection.self)
    var importSelection

    var selectedCandidate: Candidate? {
        importSelection.summary.single?.candidateKey
            .flatMap {
                importStore.selectedCandidates[$0]
            }
    }

    /// The selected folder that failed validation, once its read lands.
    var selectedInvalidCandidate: BridgeInvalidCandidateDetail? {
        importSelection.summary.single?.invalidCandidateKey
            .flatMap {
                importStore.selectedInvalidCandidates[$0]
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
                    ImportListFailedView(
                        failure: failure,
                        onRetry: { listSlot.startLoad() }
                    )
                }
                else if importStore.watchedFolders.isEmpty {
                    ImportEmptyState()
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
        .macAlert(item: $candidateActionConfirmation) { offer in
            (
                MacAlertWords(
                    Text("Replace selected metadata?"),
                    message: Text(
                        "This replaces metadata and cover choices for the selected folders. Source files and track layout are unchanged."
                    )
                ),
                [
                    MacAlertButton(
                        Text(offer.action.label(count: offer.applicable)),
                        role: .destructive
                    ) { performCandidateAction(offer) },
                    MacAlertButton(Text("Cancel"), role: .cancel),
                ]
            )
        }
    }

    /// Stop watching `path`. Its candidates leave the selection in the same
    /// write that removes them.
    func removeWatchedFolder(_ path: Data) {
        Task {
            do {
                try await importer.removeWatchedFolder(path)
            }
            catch {
                if let failure = DisplayError(error)?
                    .rewording({
                        String(localized: "Could not remove folder: \($0)")
                    })
                {
                    uiStore.showError(failure)
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
                        "\(folder.name) (\(folder.pathText))"
                    )
                )
            }
        }
    }
}

/// What one of the import view's candidate writes acts on.
enum ImportTaskKey: Hashable {
    /// A write to one candidate, by its key.
    case candidate(Data)
    /// Revealing one candidate's folders in Finder.
    case revealSources(Data)
    /// Reading every release below a folder as one.
    case combineFolder(BridgeFolderReleaseDecisionKey)
    case separateSelection
    case selectionAction
    case revealSelection
}
