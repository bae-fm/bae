import SwiftUI

/// A zone's name, set in small caps: AUTOMATIC over an empty identify area,
/// MANUAL beside the search form, RESULTS FOR over a submitted query. The
/// zones share one page, so the label is what tells them apart.
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
