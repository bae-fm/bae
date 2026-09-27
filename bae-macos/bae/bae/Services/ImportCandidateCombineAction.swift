import BaeKit
import Foundation

/// Combining the selected folders into one release. The order the folders play
/// in, their disc layout and the release name are core's — all three become the
/// combined candidate's own draft, which the pane edits and "Keep as Separate
/// Releases" undoes — and so is which folders go in: the selection core holds.
///
/// Core selects the combined release, and this reveals it. A failure leaves the
/// selection as it was, for another attempt.
@MainActor
struct ImportCandidateCombineAction {
    let selection: ImportSelection
    let uiStore: UiStore

    func run() async {
        do {
            let key = try await selection.combine()
            try Task.checkCancellation()
            uiStore.navigateToImportCandidate(key)
        }
        catch is CancellationError {}
        catch {
            uiStore.showError(error)
        }
    }
}
