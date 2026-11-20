import SwiftUI

/// A settings row: a label at the left and its value or controls at the
/// right. Inside a settings box or form the row is one line of the box.
public struct MacRow<Label: View, Content: View>: View {
    private let label: Label
    private let content: Content

    public init(
        @ViewBuilder content: () -> Content,
        @ViewBuilder label: () -> Label
    ) {
        self.content = content()
        self.label = label()
    }

    public var body: some View {
        LabeledContent {
            content
        } label: {
            label
        }
    }
}

extension MacRow where Label == Text {
    public init(
        _ title: LocalizedStringKey,
        @ViewBuilder content: () -> Content
    ) {
        self.init(content: content) { Text(title) }
    }
}

extension MacRow where Label == Text, Content == Text {
    /// A row whose right side is a line of text.
    public init<S: StringProtocol>(_ title: LocalizedStringKey, value: S) {
        self.init(title) { Text(value) }
    }
}

extension MacRow where Label == Text, Content == ToneDotValue {
    /// A row whose right side is `value` in `tone`, after a dot of it.
    public init(
        _ title: LocalizedStringKey,
        value: String,
        tone: StatusTone
    ) {
        self.init(title) { ToneDotValue(text: value, tone: tone) }
    }
}
