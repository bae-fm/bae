import BaeKit
import SwiftUI

/// The library screen's collapsing header band: the mode heading at the
/// leading edge and the mode-specific trailing controls, baseline-aligned.
/// Metrics scrub between the full and compact states off `collapseProgress`
/// (`HeaderCollapse.progress`).
struct LibraryHeader<Trailing: View>: View {
    let collapseProgress: Double
    @ViewBuilder
    let trailing: Trailing

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            LibraryModeHeading(collapseProgress: collapseProgress)
            Spacer()
            trailing
        }
        .padding(.horizontal, 40)
        .padding(.top, 40 - 32 * collapseProgress)
        // The compact bottom inset shrinks so the heading sits low in the
        // band, with enough breathing room off the content edge.
        .padding(.bottom, 24 - 16 * collapseProgress)
        .animation(.easeOut(duration: 0.15), value: collapseProgress)
    }
}

#if DEBUG
    /// Scroll the dummy list to drive the collapse through the real
    /// `HeaderCollapse` + `reportsHeaderScroll` pipeline — the same wiring
    /// the app uses, minus the library behind it.
    #Preview("Collapsing header") {
        @Previewable
        @State
        var headerCollapse = HeaderCollapse()
        VStack(spacing: 0) {
            LibraryHeader(collapseProgress: headerCollapse.progress) {
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
                        RoundedRectangle(cornerRadius: 8)
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
