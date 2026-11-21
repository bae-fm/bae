import SwiftUI

/// What pressing a `MacRow` does, which also says how it reads.
public enum MacRowPress {
    /// The row opens something: a chevron ends it.
    case opens(() -> Void)
    /// The row is itself an action that removes something.
    case removes(() -> Void)
}

/// A settings row, as the design system's `MacRow` draws one: a lead such as
/// a person's picture, the row's name with a note under it, then its value in
/// secondary text and its controls at the right, a group's space apart.
/// Inside a settings box the row is one line of the box.
public struct MacRow<Lead: View, Label: View, Content: View>: View {
    /// What pressing the row does, which also says how it reads.
    public typealias Press = MacRowPress

    private let lead: Lead
    private let label: Label
    private let content: Content
    private var value: String?
    private var note: Text?
    private var noteTone: StatusTone?
    private var chip: String?
    private var isStrong = false
    private var press: Press?

    @Environment(\.macRowPlacement)
    private var placement

    private init(lead: Lead, label: Label, content: Content) {
        self.lead = lead
        self.label = label
        self.content = content
    }

    /// The row with `lead` before its name, such as a person's picture.
    public func lead<NewLead: View>(
        @ViewBuilder _ lead: () -> NewLead
    ) -> MacRow<NewLead, Label, Content> where Lead == EmptyView {
        var row = MacRow<NewLead, Label, Content>(
            lead: lead(),
            label: label,
            content: content
        )
        row.value = value
        row.note = note
        row.noteTone = noteTone
        row.chip = chip
        row.isStrong = isStrong
        row.press = press
        return row
    }

    /// The row's value, in secondary text before its controls; nil or empty
    /// draws none.
    public func value(_ words: String?) -> MacRow {
        var copy = self
        copy.value = words
        return copy
    }

    /// A line under the row's name, in secondary text, or in `tone`'s colour
    /// when it says something in that tone, such as why an invite failed.
    public func note(_ text: Text?, tone: StatusTone? = nil) -> MacRow {
        var copy = self
        copy.note = text
        copy.noteTone = tone
        return copy
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
            Button(action: action) { pressable(chevron: true) }
                .buttonStyle(.accentPlain)
                .placesItselfInRow()
        case .removes(let action)?:
            Button(role: .destructive, action: action) {
                pressable(chevron: false).foregroundStyle(Theme.danger)
            }
            .buttonStyle(.accentPlain)
            .placesItselfInRow()
        case nil:
            // The row reads its name, and the note under it, before the
            // controls at its end: VoiceOver orders a window's elements by
            // where they are, and a control centred on two lines sits
            // between them.
            row(chevron: false)
                .accessibilityElement(children: .contain)
        }
    }

    /// The row inside its button: the button takes the whole row, its
    /// box's space round the content included, so the row presses from its
    /// edges.
    @ViewBuilder
    private func pressable(chevron: Bool) -> some View {
        if let placement {
            row(chevron: chevron).placedInRow(placement)
                .contentShape(Rectangle())
        }
        else {
            row(chevron: chevron)
        }
    }

    private func row(chevron: Bool) -> some View {
        HStack(spacing: ThemeSpace.group) {
            lead
            LabeledContent {
                HStack(spacing: ThemeSpace.group) {
                    if let value, !value.isEmpty {
                        // A value stays on one line, as the boards keep
                        // it, and the name beside it wraps instead.
                        Text(verbatim: value)
                            .foregroundStyle(Theme.macTextSecondary)
                            .fixedSize(horizontal: true, vertical: false)
                    }
                    content
                    if chevron {
                        Image(systemName: "chevron.forward")
                            .themeIcon(.small)
                            .foregroundStyle(Theme.macTextSecondary)
                    }
                }
            } label: {
                VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                    HStack(spacing: ThemeSpace.compact) {
                        if isStrong {
                            label.themeText(.strong)
                        }
                        else {
                            label
                        }
                        if let chip {
                            StatusChip(verbatim: chip)
                        }
                    }
                    if let note {
                        note
                            .themeText(.detail)
                            .foregroundStyle(
                                noteTone.map { AnyShapeStyle($0.color) }
                                    ?? AnyShapeStyle(Theme.macTextSecondary)
                            )
                    }
                }
                .accessibilitySortPriority(1)
            }
        }
        .contentShape(Rectangle())
    }
}

extension MacRow where Lead == EmptyView {
    public init(
        @ViewBuilder content: () -> Content,
        @ViewBuilder label: () -> Label
    ) {
        self.init(lead: EmptyView(), label: label(), content: content())
    }
}

extension MacRow where Lead == EmptyView, Label == Text {
    public init(
        _ title: LocalizedStringKey,
        @ViewBuilder content: () -> Content
    ) {
        self.init(content: content) { Text(title) }
    }

    /// A row whose right side is `value` in secondary text, then its
    /// controls.
    public init<S: StringProtocol>(
        _ title: LocalizedStringKey,
        value: S,
        @ViewBuilder content: () -> Content
    ) {
        self = MacRow(title, content: content).value(String(value))
    }
}

extension MacRow where Lead == EmptyView, Label == Text, Content == EmptyView {
    /// A row whose right side is `value` in secondary text.
    public init<S: StringProtocol>(_ title: LocalizedStringKey, value: S) {
        self.init(title, value: value) {}
    }
}

extension MacRow
where
    Lead == EmptyView,
    Label == Text,
    Content == ToneDotValue
{
    /// A row whose right side is `value` in `tone`, after a dot of it.
    public init(
        _ title: LocalizedStringKey,
        value: String,
        tone: StatusTone
    ) {
        self.init(title) { ToneDotValue(text: value, tone: tone) }
    }
}
