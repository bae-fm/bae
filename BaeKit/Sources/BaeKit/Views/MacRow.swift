import SwiftUI

/// A settings row: a label at the left and its value or controls at the
/// right. Inside a settings box or form the row is one line of the box.
public struct MacRow<Label: View, Content: View>: View {
    /// What pressing the row does, which also says how it reads.
    public enum Press {
        /// The row opens something: a chevron ends it.
        case opens(() -> Void)
        /// The row is itself an action that removes something.
        case removes(() -> Void)
    }

    private let label: Label
    private let content: Content
    private var chip: String?
    private var isStrong = false
    private var press: Press?

    public init(
        @ViewBuilder content: () -> Content,
        @ViewBuilder label: () -> Label
    ) {
        self.content = content()
        self.label = label()
    }

    /// A chip after the row's name, such as a person's role.
    public func chip(_ words: String?) -> MacRow {
        var copy = self
        copy.chip = words
        return copy
    }

    /// The row's name in the weight of a name that heads a row, such as a
    /// person's.
    public func strong(_ on: Bool = true) -> MacRow {
        var copy = self
        copy.isStrong = on
        return copy
    }

    /// The row as a button that does `press`.
    public func pressing(_ press: Press?) -> MacRow {
        var copy = self
        copy.press = press
        return copy
    }

    public var body: some View {
        switch press {
        case .opens(let action)?:
            Button(action: action) { row(chevron: true) }
                .buttonStyle(.accentPlain)
        case .removes(let action)?:
            Button(role: .destructive, action: action) {
                row(chevron: false).foregroundStyle(Theme.danger)
            }
            .buttonStyle(.accentPlain)
        case nil:
            row(chevron: false)
        }
    }

    private func row(chevron: Bool) -> some View {
        LabeledContent {
            HStack(spacing: ThemeSpace.related) {
                content
                if chevron {
                    Image(systemName: "chevron.forward")
                        .themeIcon(.small)
                        .foregroundStyle(.secondary)
                }
            }
        } label: {
            HStack(spacing: ThemeSpace.related) {
                if isStrong {
                    label.themeText(.rowTitle)
                }
                else {
                    label
                }
                if let chip {
                    StatusChip(verbatim: chip)
                }
            }
        }
        .contentShape(Rectangle())
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
