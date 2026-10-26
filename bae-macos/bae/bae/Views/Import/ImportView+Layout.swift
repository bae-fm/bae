import BaeKit
import SwiftUI

// MARK: - Body layout

extension ImportView {
    @ViewBuilder
    var splitContent: some View {
        HSplitView {
            candidateList
                // The floor is what the four tab labels and their count
                // badges need on one line in English; wider locales get the
                // labels' scale-down allowance on top.
                .frame(minWidth: 410, idealWidth: 420, maxWidth: 460)
            if importSelection.summary.count > 1 {
                ImportCandidateBulkSelectionPane(
                    storageCloud: storageCloud,
                    storagePinned: storagePinned,
                    onPerform: requestCandidateAction
                )
                .frame(
                    minWidth: 620,
                    maxWidth: .infinity,
                    maxHeight: .infinity
                )
            }
            else if let candidate = selectedCandidate {
                mainPane(for: candidate)
                    // The floor is the identity card's and the commit bar's.
                    // The mapping table keeps its own — it is laid out at
                    // whatever its columns need and scrolls sideways inside the
                    // pane when the pane has less than that.
                    .frame(
                        minWidth: 620,
                        maxWidth: .infinity,
                        maxHeight: .infinity
                    )
            }
            else if let invalid = selectedInvalidCandidate {
                InvalidCandidatePane(
                    detail: invalid,
                    previewingTarget: importStore.previewState.active?.target,
                    mappingActions: ImportMappingFlow.actions(
                        key: invalid.candidate.candidateKey,
                        services: mappingServices
                    ),
                    onNavigateToPlacement: {
                        uiStore.setImportCandidateTab(.skipped)
                    }
                )
                .frame(
                    minWidth: 620,
                    maxWidth: .infinity,
                    maxHeight: .infinity
                )
            }
            else if let selection = importSelection.summary.single {
                // A folder is chosen and its read has not landed yet. Blank,
                // not the placeholder: "select a folder" would be false about
                // a folder that is selected, and it was flashing on every
                // click. Never the previous candidate either — the pane reads
                // the selected key, which holds nothing until this one lands.
                PendingReadPane(selection: selection)
                    .frame(
                        minWidth: 620,
                        maxWidth: .infinity,
                        maxHeight: .infinity
                    )
            }
            else {
                ContentUnavailableView {
                    Text("Select a folder")
                } description: {
                    Text("Choose a scanned folder to search for metadata")
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
