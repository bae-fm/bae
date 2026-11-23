import BaeKit
import SwiftUI

/// The library's collapsing header: the mode heading and the mode's controls,
/// sized between full and compact by `collapseProgress`.
struct MacLibraryHeading<Trailing: View>: View {
    let collapseProgress: Double
    /// Span the window instead of centering in the shared capped column
    /// (`Config.libraryFullWidth`).
    let fullWidth: Bool
    @ViewBuilder
    let trailing: Trailing

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            LibraryModeHeading(collapseProgress: collapseProgress)
            Spacer()
            HStack(spacing: ThemeSpace.related) {
                trailing
            }
            .alignmentGuide(.firstTextBaseline) { dimensions in
                // The Albums board aligns the heading's baseline with the
                // bottom of the 14-point checkbox. Keep the row anchored
                // there as its controls change.
                dimensions[VerticalAlignment.center] + 7
            }
        }
        // Lines up with the album art: container padding plus the card's inset.
        .padding(
            .horizontal,
            LibraryContentContainer.horizontalPadding + ThemeSpace.compact
        )
        .padding(
            .top,
            collapsing(from: 2 * ThemeSpace.page, to: ThemeSpace.group)
        )
        // Shrinks on collapse so the compact heading sits low in the band.
        .padding(
            .bottom,
            collapsing(from: ThemeSpace.page, to: ThemeSpace.group)
        )
        // The content's container, so the header lines up with it at any width.
        .libraryContentContainer(fullWidth: fullWidth)
        .animation(.easeOut(duration: 0.15), value: collapseProgress)
    }

    /// A length from its full-size value to its collapsed one.
    private func collapsing(from full: CGFloat, to collapsed: CGFloat)
        -> CGFloat
    {
        full + (collapsed - full) * collapseProgress
    }
}
