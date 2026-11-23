import BaeKit
import SwiftUI

/// A settings row whose content is a boxed field with its Save button: the
/// label centred on the field, not at the top of the box, which is where
/// `LabeledContent` puts it beside a field taller than a line of text. With
/// `labelWidth`, the label takes a column of that width and the field fills
/// the rest of the row.
struct MacFieldRow<Content: View>: View {
    private enum LabelPlacement {
        case visible(LocalizedStringKey, width: CGFloat?)
        case alignedField
    }

    private let label: LabelPlacement
    private let content: Content

    init(
        _ label: LocalizedStringKey,
        labelWidth: CGFloat? = nil,
        @ViewBuilder content: () -> Content
    ) {
        self.label = .visible(label, width: labelWidth)
        self.content = content()
    }

    /// The field stands in for a row's name and shares its text inset.
    init(@ViewBuilder aligningText content: () -> Content) {
        self.label = .alignedField
        self.content = content()
    }

    @ViewBuilder
    var body: some View {
        switch label {
        case .alignedField:
            HStack(spacing: ThemeSpace.related) { content }
                .padding(.leading, -ThemeSpace.fieldInset)
                .settingsRowInset(ThemeSpace.compact)
        case .visible(let title, let width):
            HStack(alignment: .center) {
                if let width {
                    Text(title)
                        .frame(width: width, alignment: .leading)
                    HStack(alignment: .center, spacing: ThemeSpace.related) {
                        content
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                else {
                    Text(title)
                    Spacer(minLength: ThemeSpace.group)
                    HStack(alignment: .center, spacing: ThemeSpace.compact) {
                        content
                    }
                }
            }
        }
    }
}
