import BaeKit
import SwiftUI

/// The slot under a grid row: the album detail when the row holds the selected
/// album, otherwise a zero-height placeholder that keeps the row's identity.
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
        RoundedRectangle(cornerRadius: ThemeRadius.card)
            .fill(Theme.surface)
            .frame(height: 120)
            .overlay(Text(verbatim: "Expansion for \(id)"))
            .padding(.vertical, ThemeSpace.related)
    }

    #Preview("Album Expansion Slot") {
        VStack(spacing: 0) {
            AlbumExpansionSlot(
                selectedId: "album-0",
                expansionContent: previewExpansion
            )
            AlbumExpansionSlot(
                selectedId: nil,
                expansionContent: previewExpansion
            )
        }
        .padding()
        .frame(width: 480)
    }
#endif
