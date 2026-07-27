import BaeKit
import SwiftUI

// MARK: - Body layout

extension ImportView {
    @ViewBuilder
    var splitContent: some View {
        HSplitView {
            candidateList
                .frame(minWidth: 280, idealWidth: 392, maxWidth: 460)
            if let candidate = selectedCandidate {
                mainPane(for: candidate)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
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
