import BaeKit
import SwiftUI

/// A settings row whose content is a boxed field and optional button.
/// With `labelWidth`, the label sits on the field's first line, even when
/// an error or note extends below the field.
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
            HStack(
                alignment: width == nil ? .center : .top,
                spacing: width == nil ? nil : ThemeSpace.group
            ) {
                if let width {
                    Text(title)
                        .frame(width: width, alignment: .leading)
                        .padding(.top, ThemeSpace.related)
                        .accessibilitySortPriority(1)
                    HStack(alignment: .top, spacing: ThemeSpace.related) {
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
