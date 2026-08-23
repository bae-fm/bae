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
}
