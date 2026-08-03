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

    /// The identity the selected candidate's row already carries while the
    /// pane still shows nothing settled. Non-`nil` exactly when the pane
    /// should open on that identity without a click.
    var pickedResume: BridgeIdentityPick? {
        guard let candidate = selectedCandidate else {
            return nil
        }
        return ImportSearchFlow.pickedResume(
            candidate: candidate,
            row: importStore.triageRow(forKey: candidate.key)
        )
    }

    /// Apply the row's decided identity — the query half of the flow, seeding
    /// the pane from the stored decision without re-persisting it.
    func applyPickedResume(_ picked: BridgeIdentityPick) {
        guard let key = uiStore.selectedFolderCandidate else {
            return
        }
        ImportSearchFlow.refreshDecidedIdentity(
            importer: importer,
            importStore: importStore,
            key: key,
            pick: picked
        )
    }
}
