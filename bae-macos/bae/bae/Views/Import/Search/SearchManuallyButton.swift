import BaeKit
import SwiftUI

/// Opens the search with the cursor in its first field, for a section with
/// nothing to pick from.
struct SearchManuallyButton: View {
    let action: () -> Void

    @State
    private var isHovered = false

    var body: some View {
        Button(action: action) {
            Text("Search manually")
                .themeText(.strong)
                .foregroundStyle(.primary)
                .padding(.horizontal, 14)
                .frame(height: 26)
                .background(
                    isHovered ? Theme.pressed : Theme.hover,
                    in: RoundedRectangle(cornerRadius: ThemeRadius.control)
                )
                .contentShape(
                    RoundedRectangle(cornerRadius: ThemeRadius.control)
                )
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
    }
}
