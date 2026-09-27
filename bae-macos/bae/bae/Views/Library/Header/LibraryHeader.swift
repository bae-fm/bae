import BaeKit
import SwiftUI

/// The library's collapsing header: the mode heading and the mode's controls,
/// sized between full and compact by `collapseProgress`.
struct LibraryHeader<Trailing: View>: View {
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
            trailing
        }
        // Lines up with the album art: container padding plus the card's inset.
        .padding(
            .horizontal,
            LibraryContentContainer.horizontalPadding + 6
        )
        .padding(.top, 66 - 52 * collapseProgress)
        // Shrinks on collapse so the compact heading sits low in the band.
        .padding(.bottom, 32 - 20 * collapseProgress)
        // The content's container, so the header lines up with it at any width.
        .libraryContentContainer(fullWidth: fullWidth)
        .animation(.easeOut(duration: 0.15), value: collapseProgress)
    }
}

#if DEBUG
    /// Scrolling the list drives the collapse through the app's real wiring.
    #Preview("Collapsing header") {
        @Previewable
        @State
        var headerCollapse = HeaderCollapse()
        VStack(spacing: 0) {
            LibraryHeader(
                collapseProgress: headerCollapse.progress,
                fullWidth: false
            ) {
                SortCriteriaRow(
                    criteria: .constant([
                        BridgeSortCriterion(
                            field: .artist,
                            direction: .ascending
                        )
                    ])
                )
            }
            ScrollView {
                LazyVStack(spacing: 12) {
                    ForEach(0..<80, id: \.self) { index in
                        RoundedRectangle(cornerRadius: ThemeRadius.control)
                            .fill(Theme.surface)
                            .frame(height: 48)
                            .overlay(alignment: .leading) {
                                Text(verbatim: "Row \(index)")
                                    .foregroundStyle(.secondary)
                                    .padding(.leading, 16)
                            }
                    }
                }
                .padding(.horizontal, 40)
                .padding(.bottom)
            }
            .reportsHeaderScroll(id: "preview")
        }
        .environment(headerCollapse)
        .environment(UiStore())
        .background(Theme.background)
        .frame(width: 760, height: 560)
    }
#endif
