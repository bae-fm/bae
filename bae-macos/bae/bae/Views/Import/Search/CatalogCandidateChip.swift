import BaeKit
import SwiftUI

/// A catalog number extraction found and the run is not looking up: the
/// number, outlined and dimmed. Clicking it puts the number
/// into the run and every provider looks it up.
struct CatalogCandidateChip: View {
    let candidate: BridgeCatalogCandidate
    let onActivate: () -> Void

    var body: some View {
        Button(action: onActivate) {
            IdentifierChip(
                label: String(localized: "Catalog #"),
                value: candidate.value,
                style: .outlined
            )
        }
        .buttonStyle(.plain)
        .help("Look this catalog number up")
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Catalog numbers waiting to be used") {
        FlowLayout(spacing: ThemeSpace.compact) {
            ForEach(PreviewData.catalogCandidates, id: \.value) { candidate in
                CatalogCandidateChip(candidate: candidate, onActivate: {})
            }
        }
        .padding()
        .frame(width: 400)
        .windowBackground()
    }
#endif
