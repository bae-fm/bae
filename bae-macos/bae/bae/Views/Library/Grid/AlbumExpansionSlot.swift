import BaeKit
import SwiftUI

/// The full-width detail slot rendered under a grid row: it holds the album
/// detail expansion when the row contains the currently-selected album, and an
/// inert zero-height placeholder otherwise (so the row keeps its identity).
struct AlbumExpansionSlot<ExpansionContent: View>: View {
    let selectedId: String?
    let expansionContent: (String) -> ExpansionContent

    var body: some View {
        ZStack {
            Color.clear.frame(height: 0)
            selectedId.map { id in
                expansionContent(id)
                    .transition(.opacity)
            }
        }
    }
}

#if DEBUG
    private func previewExpansion(_ id: String) -> some View {
        RoundedRectangle(cornerRadius: 10)
            .fill(Theme.surface)
            .frame(height: 120)
            .overlay(Text(verbatim: "Expansion for \(id)"))
            .padding(.vertical, 8)
    }

    #Preview("Album Expansion Slot") {
        VStack(spacing: 0) {
            // Selected: the slot holds its expansion content.
            AlbumExpansionSlot(
                selectedId: "album-0",
                expansionContent: previewExpansion
            )
            // Unselected: an inert zero-height placeholder.
            AlbumExpansionSlot(
                selectedId: nil,
                expansionContent: previewExpansion
            )
        }
        .padding()
        .frame(width: 480)
    }
#endif
