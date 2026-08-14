import BaeKit
import SwiftUI

// MARK: - Candidate selection

extension ImportView {
    var candidateSelectionBinding: Binding<Set<String>> {
        Binding(
            get: { uiStore.selectedFolderCandidates },
            set: { keys in
                uiStore.setFolderCandidateSelection(keys)
                guard keys.count == 1,
                    let key = keys.first,
                    let candidate = importStore.folderCandidates[key]
                else { return }
                identifyCandidateOnFirstSelection(candidate)
            },
        )
    }

    private func identifyCandidateOnFirstSelection(_ candidate: Candidate) {
        guard case .folder = candidate.source else {
            return
        }

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
        guard uiStore.selectedFolderCandidates.count == 1,
            let key = uiStore.selectedFolderCandidates.first
        else {
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
