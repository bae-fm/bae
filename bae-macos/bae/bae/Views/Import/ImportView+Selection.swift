import BaeKit
import SwiftUI

// MARK: - Candidate selection

extension ImportView {
    /// The sidebar's selection. Reporting it to `UiStore` is what opens the
    /// per-candidate read behind each key, which is also where a newly
    /// selected folder's identification starts.
    var candidateSelectionBinding: Binding<Set<String>> {
        Binding(
            get: { uiStore.selectedFolderCandidates },
            set: { keys in
                uiStore.setFolderCandidateSelection(keys)
            },
        )
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
            row: candidate.row
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
