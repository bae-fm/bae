import BaeKit
import SwiftUI

/// One chip per state narrowing the list, each with an ✕ that clears just
/// that state. Scrolls sideways when they outgrow the field.
struct PendingFilterChips: View {
    /// The states narrowing the list, in the menu's order.
    let filters: [BridgePendingState]
    let onClear: (BridgePendingState) -> Void

    var body: some View {
        ViewThatFits(in: .horizontal) {
            chips
            ScrollView(.horizontal, showsIndicators: false) {
                chips
            }
        }
    }

    private var chips: some View {
        HStack(spacing: ThemeSpace.inline) {
            ForEach(filters, id: \.self) { filter in
                StatusChip(
                    verbatim: filter.label,
                    tone: .accent,
                    removal: StatusChip.Removal(
                        Text("Stop Filtering by \(filter.label)")
                    ) {
                        onClear(filter)
                    }
                )
            }
        }
    }
}

#if DEBUG
    #Preview("Pending filter chips") {
        PendingFilterChips(
            filters: [.needsYou, .lookupError, .importError],
            onClear: { _ in }
        )
        .padding()
        .frame(width: 280)
        .windowBackground()
    }
#endif
