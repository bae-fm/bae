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
            .noteText(above: ThemeSpace.line)
    }
}

extension View {
    /// Sets a note's text: secondary detail text starting where a box's row
    /// labels start, with `above` of space over it.
    func noteText(above: CGFloat) -> some View {
        themeText(.detail)
            .foregroundStyle(Theme.macTextSecondary)
            .padding(.horizontal, RowMetrics.inset)
            .padding(.top, above)
            .fixedSize(horizontal: false, vertical: true)
    }
}
