import BaeKit
import SwiftUI

// MARK: - Candidate selection

extension ImportView {
    /// The sidebar's selected keys feed the dedicated bulk-action query.
    var candidateSelectionBinding: Binding<Set<String>> {
        Binding(
            get: { uiStore.selectedFolderCandidates },
            set: { keys in
                uiStore.setFolderCandidateSelection(keys)
            },
        )
    }
}
