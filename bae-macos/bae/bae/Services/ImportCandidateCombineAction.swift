import BaeKit
import Foundation

/// Combining the selected folders into one release. The order the folders play
/// in, their disc layout and the release name are core's — all three become the
/// combined candidate's own draft, which the pane edits and "Separate Folders"
/// undoes — so the only thing this carries over is the selection.
///
/// The combined release is selected and revealed once it exists. A failure
/// leaves the selection as it was, for another attempt.
@MainActor
struct ImportCandidateCombineAction {
    let importer: Importer
    let uiStore: UiStore
    let listSlot: ImportListSlot

    func run() async {
        do {
            let key = try await importer.combineCandidates(
                uiStore.selectedFolderCandidates.sorted()
            )
            try Task.checkCancellation()
            uiStore.setFolderCandidateSelection([key])
            listSlot.requestCandidateReveal(key)
        }
        catch is CancellationError {}
        catch {
            uiStore.showError(error)
        }
    }
}
