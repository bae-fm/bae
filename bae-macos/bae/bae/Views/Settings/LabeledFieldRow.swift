import BaeKit
import SwiftUI

/// A settings row whose content is a boxed field with its Save button: the
/// label on the field's text line, not at the top of the box, which is where
/// `LabeledContent` puts it beside a field taller than a line of text.
struct LabeledFieldRow<Content: View>: View {
    private let label: LocalizedStringKey
    private let content: Content

    init(_ label: LocalizedStringKey, @ViewBuilder content: () -> Content) {
        self.label = label
        self.content = content()
    }

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(label)
            Spacer(minLength: ThemeSpace.group)
            HStack(
                alignment: .firstTextBaseline,
                spacing: ThemeSpace.related
            ) {
                content
            }
        }
    }
}
