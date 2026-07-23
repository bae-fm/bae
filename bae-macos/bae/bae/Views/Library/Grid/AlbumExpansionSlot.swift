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
