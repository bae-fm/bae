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
            if let candidate = selectedCandidate {
                mainPane(for: candidate)
                    // The floor keeps the mapping table's columns readable —
                    // below it the split pane scrolls the window, not the
                    // table into per-character wrapping.
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
