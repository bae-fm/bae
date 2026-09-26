import BaeKit
import Foundation

/// Combining the selected folders into one release. The order the folders play
/// in, their disc layout and the release name are core's — all three become the
/// combined candidate's own draft, which the pane edits and "Keep as Separate
/// Releases" undoes — so the only thing this carries over is the selection.
///
/// The combined release is selected and revealed once it exists. A failure
/// leaves the selection as it was, for another attempt.
@MainActor
struct ImportCandidateCombineAction {
    let importer: Importer
    let uiStore: UiStore
    /// The releases to read as one, in the order the selection offers them.
    let keys: [String]

    func run() async {
        do {
            let key = try await importer.combineCandidates(keys)
            try Task.checkCancellation()
            uiStore.navigateToImportCandidate(key, selecting: [key])
        }
        catch is CancellationError {}
        catch {
            uiStore.showError(error)
        }
    }
}
