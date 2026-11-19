import BaeKit
import SwiftUI

/// A loose paragraph between boxes: 11.5 over 15 in secondary text, set in
/// from the boxes' edges.
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
            .padding(.horizontal, ThemeSpace.inline)
            .padding(.top, ThemeSpace.compact)
            .fixedSize(horizontal: false, vertical: true)
    }
}
