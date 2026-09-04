import SwiftUI

/// The one thing a folder nobody has looked up offers: start identification.
/// Bordered so it reads as the zone's control rather than a link. What
/// Identify reads is the button's help, not a paragraph beside it.
struct IdentifyAutomaticallyButton: View {
    let action: () -> Void

    @State
    private var isHovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 7) {
                Image(systemName: "touchid")
                    .font(.system(size: 13))
                Text("Identify automatically")
                    .font(.system(size: 13, weight: .semibold))
            }
            .foregroundStyle(Color.accentColor)
            .padding(.horizontal, 14)
            .frame(height: 30)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(Color.accentColor.opacity(isHovered ? 0.08 : 0))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .strokeBorder(Color.accentColor.opacity(0.35), lineWidth: 1)
            )
            .contentShape(RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
        .help(
            "Identify reads the folder's disc TOC, barcode, and catalog number and asks both sources."
        )
    }
}
