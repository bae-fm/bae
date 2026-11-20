import BaeKit
import SwiftUI

// MARK: - Body layout

extension MacImportWindow {
    /// What the pane beside the list keeps when the main window is at its
    /// smallest width, 900 points, and the list is at its 410-point floor.
    /// Whatever the pane holds fits this width, or wraps or scrolls inside it.
    static let paneFloor: CGFloat = 489

    @ViewBuilder
    var splitContent: some View {
        HSplitView {
            candidateList
                // The floor is what the four tab labels and their count
                // badges need on one line in English; wider locales get the
                // labels' scale-down allowance on top.
                .frame(minWidth: 410, idealWidth: 420, maxWidth: 460)
            if importSelection.summary.count > 1 {
                MacImportBulk(
                    importDestination: importDestination,
                    onPerform: requestCandidateAction
                )
                .frame(
                    minWidth: Self.paneFloor,
                    maxWidth: .infinity,
                    maxHeight: .infinity
                )
            }
            else if let candidate = selectedCandidate {
                mainPane(for: candidate)
                    // The identify bar wraps its destination and Import onto a
                    // second line when the pane is narrow. The mapping table
                    // keeps its columns' floors and scrolls sideways inside the
                    // pane when the pane has less than that.
                    .frame(
                        minWidth: Self.paneFloor,
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
                    minWidth: Self.paneFloor,
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
                Color.clear
                    .frame(
                        minWidth: Self.paneFloor,
                        maxWidth: .infinity,
                        maxHeight: .infinity
                    )
            }
            else {
                MacContentUnavailable(
                    title: "Select a folder",
                    description:
                        "Choose a scanned folder to search for metadata"
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
