import BaeKit
import SwiftUI

/// The short words macOS shows under the pointer after it rests on a
/// control: SwiftUI's `.help` on the control. `showing` draws those words
/// under the control in place of the system's tag, which a shot cannot
/// capture.
struct MacHelpTag<Content: View>: View {
    let text: String
    var showing = false
    @ViewBuilder
    let content: () -> Content

    /// How far in from the control's left edge the tag's corner sits.
    private static var inset: CGFloat { 10 }
    /// The gap between the control and the tag.
    private static var gap: CGFloat { 6 }

    var body: some View {
        content()
            .help(text)
            .overlay(alignment: .bottomLeading) {
                if showing {
                    // A line along the control's bottom edge, which the tag
                    // hangs from.
                    Color.clear
                        .frame(height: 0)
                        .overlay(alignment: .topLeading) {
                            tag
                                .padding(.leading, Self.inset)
                                .padding(.top, Self.gap)
                        }
                }
            }
    }

    private var tag: some View {
        Text(text)
            .themeText(.cell)
            .foregroundStyle(Theme.textPrimary)
            .lineLimit(1)
            .fixedSize()
            .padding(.vertical, ThemeSpace.line)
            .padding(.horizontal, ThemeSpace.compact)
            .background(
                Theme.surfaceElevated,
                in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
            )
            .overlay {
                RoundedRectangle(cornerRadius: ThemeRadius.chip)
                    .strokeBorder(Theme.hairlineStrong, lineWidth: 1)
            }
            .shadow(color: Theme.shadow, radius: 6, y: 4)
            .allowsHitTesting(false)
    }
}
