import SwiftUI

/// A sheet's line under its title, in secondary text: what the sheet asks for
/// or reports, as an alert's message sits under its title.
struct SheetMessage: View {
    private let text: LocalizedStringKey

    init(_ text: LocalizedStringKey) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .themeText(.body)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
    }
}
