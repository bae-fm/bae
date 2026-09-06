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
            if uiStore.selectedFolderCandidates.count > 1 {
                ImportCandidateBulkSelectionPane(
                    storageCloud: $storageCloud,
                    storagePinned: $storagePinned,
                    onPerform: performCandidateAction,
                    onCombine: reviewSelectedCombination
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
            else if let key = uiStore.selectedFolderCandidates.first {
                // A folder is chosen and its read has not landed yet. Blank,
                // not the placeholder: "select a folder" would be false about
                // a folder that is selected, and it was flashing on every
                // click. Never the previous candidate either — the pane reads
                // the selected key, which holds nothing until this one lands.
                ImportCandidatePendingPane(candidateKey: key)
                    .frame(
                        minWidth: 620,
                        maxWidth: .infinity,
                        maxHeight: .infinity
                    )
            }
            else {
                ContentUnavailableView(
                    "Select a folder",
                    systemImage: "folder",
                    description: Text(
                        "Choose a scanned folder to search for metadata"
                    ),
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    @ViewBuilder
    var documentOverlay: some View {
        if let doc = documentContent {
            Color.black.opacity(0.5)
                .ignoresSafeArea()
                .onTapGesture { documentContent = nil }
            DocumentViewerView(
                name: doc.name,
                text: doc.text,
                onClose: { documentContent = nil }
            )
            .frame(width: 750, height: 600)
            .background(Theme.surface)
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .shadow(radius: 20)
        }
    }
}

/// The pane for a selected candidate whose read has not been delivered yet.
///
/// Empty rather than a placeholder, and empty rather than a spinner: the read
/// almost always lands within a frame or two, and a spinner that appears and
/// leaves that fast is a flash of its own. One shows only if the wait outlasts
/// `spinnerDelay`, which is the case worth reporting — a folder whose read is
/// genuinely slow.
private struct ImportCandidatePendingPane: View {
    /// Restarts the wait when the selection moves to another folder while one
    /// is still pending.
    let candidateKey: String

    private static let spinnerDelay = Duration.milliseconds(150)

    @State
    private var waitedLongEnough = false

    var body: some View {
        VStack {
            if waitedLongEnough {
                ProgressView().controlSize(.small)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task(id: candidateKey) {
            waitedLongEnough = false
            try? await Task.sleep(for: Self.spinnerDelay)
            guard !Task.isCancelled else { return }
            waitedLongEnough = true
        }
    }
}
