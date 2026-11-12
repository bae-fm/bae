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
        .buttonStyle(.accentPlain)
        .help("Look this catalog number up")
    }
}

#if DEBUG
    // MARK: - Previews

#endif
