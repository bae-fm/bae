import BaeKit
import SwiftUI

/// A settings row whose content is a boxed field with its Save button: the
/// label centred on the field, not at the top of the box, which is where
/// `LabeledContent` puts it beside a field taller than a line of text. With
/// `labelWidth`, the label takes a column of that width and the field fills
/// the rest of the row.
struct MacFieldRow<Content: View>: View {
    private let label: LocalizedStringKey
    private let labelWidth: CGFloat?
    private let content: Content

    init(
        _ label: LocalizedStringKey,
        labelWidth: CGFloat? = nil,
        @ViewBuilder content: () -> Content
    ) {
        self.label = label
        self.labelWidth = labelWidth
        self.content = content()
    }

    var body: some View {
        HStack(alignment: .center) {
            if let labelWidth {
                Text(label)
                    .frame(width: labelWidth, alignment: .leading)
                HStack(alignment: .center, spacing: ThemeSpace.related) {
                    content
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            else {
                Text(label)
                Spacer(minLength: ThemeSpace.group)
                HStack(alignment: .center, spacing: ThemeSpace.compact) {
                    content
                }
            }
        }
    }
}
