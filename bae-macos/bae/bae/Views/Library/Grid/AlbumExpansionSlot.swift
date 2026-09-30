import BaeKit
import SwiftUI

/// The open album's detail in the grid. A lazy grid has no slot wider than a
/// column, so the detail takes the first slot of its own row and is drawn
/// across the whole row; the row's other slots are empty.
struct AlbumExpansionSlot<ExpansionContent: View>: View {
    let albumId: String
    /// The width of the one column the slot occupies.
    let slotWidth: CGFloat
    /// The width of the row the detail is drawn across.
    let rowWidth: CGFloat
    let expansionContent: (String) -> ExpansionContent

    var body: some View {
        expansionContent(albumId)
            .frame(width: rowWidth)
            .frame(width: slotWidth, alignment: .leading)
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
        AlbumExpansionSlot(
            albumId: "album-0",
            slotWidth: 140,
            rowWidth: 448,
            expansionContent: previewExpansion
        )
        .frame(width: 448, alignment: .leading)
        .padding()
        .frame(width: 480)
    }
#endif
