import BaeKit
import SwiftUI

/// A sheet's line under its title, in secondary text: what the sheet asks for
/// or reports, as an alert's message sits under its title.
struct SheetMessage: View {
    private let text: Text

    init(_ text: Text) {
        self.text = text
    }

    var body: some View {
        text
            .themeText(.body)
            .foregroundStyle(Theme.textSecondary)
            .fixedSize(horizontal: false, vertical: true)
    }
}
