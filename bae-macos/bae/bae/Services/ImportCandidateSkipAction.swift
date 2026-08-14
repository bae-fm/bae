import BaeKit
import Foundation
import os.log

private let importCandidateSkipLogger = Logger.bae("ImportCandidateSkipAction")

/// The single bulk-skip operation used by the mapping pane and Command-E.
/// Eligibility is read from the current core projection at invocation time;
/// completion removes only the selection the operation captured, preserving
/// any rows selected while its calls were in flight.
@MainActor
struct ImportCandidateSkipAction {
    let importer: Importer
    let importStore: ImportStore
    let uiStore: UiStore

    var isEnabled: Bool {
        !eligibleKeys.isEmpty
    }

    func perform() async {
        let selectedKeys = uiStore.selectedFolderCandidates
        let keys = importStore.skippableCandidateKeys(in: selectedKeys)
        guard !keys.isEmpty else { return }

        var firstFailure: DisplayError?
        for key in keys {
            do {
                try await importer.setCandidateSkipped(key, true)
            }
            catch is CancellationError {
                return
            }
            catch {
                importCandidateSkipLogger.error(
                    "could not skip candidate \(key): \(String(reflecting: error))"
                )
                if firstFailure == nil {
                    firstFailure = DisplayError(error)
                }
            }
        }
        uiStore.removeFolderCandidateSelection(selectedKeys)

        if let firstFailure {
            uiStore.showError(firstFailure)
        }
    }

    private var eligibleKeys: [String] {
        importStore.skippableCandidateKeys(
            in: uiStore.selectedFolderCandidates
        )
    }
}
