import BaeKit
import SwiftUI

/// A picker's face: the chosen value, or the form's empty mark.
struct FactMenuLabel: View {
    let text: String?

    /// The form's mark for a field with no value. A symbol, not a sentence,
    /// so it is the same in every language.
    static let emptyMark = "\u{2014}"

    /// The words the face shows: the value, or the form's dash.
    static func shown(_ text: String?) -> String {
        text ?? emptyMark
    }

    var body: some View {
        Text(Self.shown(text))
            .themeText(.body)
            .foregroundStyle(
                text == nil ? Theme.textTertiary : Theme.textPrimary
            )
    }
}
