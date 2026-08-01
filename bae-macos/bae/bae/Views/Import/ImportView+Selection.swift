import BaeKit
import SwiftUI

// MARK: - Candidate selection

extension ImportView {
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
        guard case .folder = candidate.source else {
            return
        }

        uiStore.selectFolderCandidate(candidate.key)

        // Identify gate: only kick off on the first selection. Subsequent
        // re-selects (including back-to-identify from Confirming) keep the
        // last state. Identify also starts extraction, which streams the
        // candidate's signals (disc ID, barcodes, classified text).
        if case .idle = candidate.identifyState {
            importer.autoIdentifyFolder(candidate.key)
        }
    }

    /// The match the selected candidate's Ready row settles on while the pane
    /// is still in the identify phase with nothing picked. Non-`nil` exactly
    /// when the pane should open on that release without a click.
    var readyAutoPick: BridgeMatchedRelease? {
        guard let candidate = selectedCandidate else {
            return nil
        }
        return ImportSearchFlow.readyAutoPick(
            candidate: candidate,
            row: importStore.triageRow(forKey: candidate.key)
        )
    }

    /// Open the selected candidate's pane on `matched` — the same pick-and-
    /// prefetch a search-sheet row click runs.
    func applyReadyAutoPick(_ matched: BridgeMatchedRelease) {
        guard let key = uiStore.selectedFolderCandidate else {
            return
        }
        ImportSearchFlow.prefetchAndConfirm(
            library: library,
            importStore: importStore,
            key: key,
            releaseId: matched.releaseId,
            source: matched.evidence.source
        )
    }
}
