import BaeKit
import SwiftUI

/// A picker's face: the chosen value, or the form's empty mark.
struct FactMenuLabel: View {
    let text: String?

    /// The words the face shows: the value, or the form's dash.
    static func shown(_ text: String?) -> String {
        text ?? "\u{2014}"
    }

    var body: some View {
        Text(Self.shown(text))
            .themeText(.body)
            .foregroundStyle(
                text == nil ? Theme.textTertiary : Theme.textPrimary
            )
    }
}
