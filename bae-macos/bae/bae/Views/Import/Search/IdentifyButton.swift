import BaeKit
import SwiftUI

/// The one thing a folder nobody has looked up offers: start identification.
/// Filled, so it reads as the section's control rather than a link.
struct IdentifyButton: View {
    let action: () -> Void

    @Environment(\.accentChoice)
    private var accent

    var body: some View {
        Button(action: action) {
            Text("Identify")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(.white)
                .padding(.horizontal, 14)
                .frame(height: 26)
                .background(
                    accent.buttonColor,
                    in: RoundedRectangle(cornerRadius: 6)
                )
                .contentShape(RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
    }
}

/// The way out of a section with nothing to pick from: open the search with
/// the cursor in its first field. Quiet, so it offers rather than urges.
struct SearchManuallyButton: View {
    let action: () -> Void

    @State
    private var isHovered = false

    var body: some View {
        Button(action: action) {
            Text("Search manually")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(.primary)
                .padding(.horizontal, 14)
                .frame(height: 26)
                .background(
                    Color.primary.opacity(isHovered ? 0.12 : 0.08),
                    in: RoundedRectangle(cornerRadius: 6)
                )
                .contentShape(RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
    }
}
