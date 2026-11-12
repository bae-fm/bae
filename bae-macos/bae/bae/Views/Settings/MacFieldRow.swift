import BaeKit
import SwiftUI

/// A settings row whose content is a boxed field with its Save button: the
/// label centred on the field, not at the top of the box, which is where
/// `LabeledContent` puts it beside a field taller than a line of text.
struct MacFieldRow<Content: View>: View {
    private let label: LocalizedStringKey
    private let content: Content

    init(_ label: LocalizedStringKey, @ViewBuilder content: () -> Content) {
        self.label = label
        self.content = content()
    }

    var body: some View {
        HStack(alignment: .center) {
            Text(label)
            Spacer(minLength: ThemeSpace.group)
            HStack(alignment: .center, spacing: ThemeSpace.compact) {
                content
            }
        }
    }
}
