import SwiftUI

/// A section's name, set in small caps: AUTOMATIC and SEARCH on the
/// accordion's headers. The sections share one page, so the label is what
/// tells them apart.
struct FindOnlineCapsLabel: View {
    let text: LocalizedStringKey

    init(_ text: LocalizedStringKey) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .font(.system(size: 10, weight: .bold))
            .tracking(1.2)
            .textCase(.uppercase)
            .foregroundStyle(.tertiary)
    }
}
