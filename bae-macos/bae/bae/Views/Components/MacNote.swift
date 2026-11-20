import BaeKit
import SwiftUI

/// A loose paragraph between boxes: 11.5 over 15 in secondary text, starting
/// where a box's row labels start, with the 2pt the design system gives it
/// above its text (the section's 6pt gap makes 8).
struct MacNote: View {
    private let text: LocalizedStringKey

    init(_ text: LocalizedStringKey) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .themeText(.detail)
            .lineSpacing(ThemeSpace.line / 2)
            .foregroundStyle(.secondary)
            .padding(.horizontal, RowMetrics.inset)
            .padding(.top, ThemeSpace.line)
            .fixedSize(horizontal: false, vertical: true)
    }
}
