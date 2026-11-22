import SwiftUI

/// What pressing a `MacRow` does, which also says how it reads.
public enum MacRowPress {
    /// The row opens something: a chevron ends it.
    case opens(() -> Void)
    /// The label is a button, with the row's other controls beside it.
    case label(showsChevron: Bool, action: () -> Void)
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
    private var selection: Bool?

    @Environment(\.accentChoice)
    private var accent
    @Environment(\.macRowPlacement)
    private var placement
    @Environment(\.isEnabled)
    private var isEnabled

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
        row.selection = selection
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

    /// Draws a choice row, filling it when selected.
    public func selected(_ selected: Bool) -> MacRow {
        var copy = self
        copy.selection = selected
        return copy
    }

    public var body: some View {
        rowAppearance.opacity(isEnabled ? 1 : ThemeOpacity.off)
    }

    @ViewBuilder
    private var rowAppearance: some View {
        if let selection {
            rowControl
                .foregroundStyle(selection ? Theme.onFill : Theme.textPrimary)
                .background(selection ? accent.buttonColor : Color.clear)
        }
        else {
            rowControl
        }
    }

    @ViewBuilder
    private var rowControl: some View {
        switch press {
        case .opens(let action)?:
            Button(action: action) { rowWithPlacement(chevron: true) }
                .buttonStyle(.accentPlain)
                .placesItselfInRow()
        case .label(let chevron, let action)?:
            displayRow(chevron: chevron, labelAction: action)
                .accessibilityElement(children: .contain)
        case .removes(let action)?:
            Button(role: .destructive, action: action) {
                rowWithPlacement(chevron: false).foregroundStyle(Theme.danger)
            }
            .buttonStyle(.accentPlain)
            .placesItselfInRow()
        case nil:
            // The row reads its name, and the note under it, before the
            // controls at its end: VoiceOver orders a window's elements by
            // where they are, and a control centred on two lines sits
            // between them.
            displayRow(chevron: false)
                .accessibilityElement(children: .contain)
        }
    }

    /// A button and a selection fill both reach the row's edges.
    @ViewBuilder
    private func rowWithPlacement(
        chevron: Bool,
        labelAction: (() -> Void)? = nil
    ) -> some View {
        let content = row(chevron: chevron, labelAction: labelAction)
        if let placement {
            content.placedInRow(placement)
                .contentShape(Rectangle())
        }
        else {
            content
        }
    }

    @ViewBuilder
    private func displayRow(
        chevron: Bool,
        labelAction: (() -> Void)? = nil
    ) -> some View {
        if selection != nil {
            rowWithPlacement(chevron: chevron, labelAction: labelAction)
                .placesItselfInRow()
        }
        else {
            row(chevron: chevron, labelAction: labelAction)
        }
    }

    private var secondaryColor: Color {
        selection == true ? Theme.onFillSecondary : Theme.macTextSecondary
    }

    private func row(
        chevron: Bool,
        labelAction: (() -> Void)? = nil
    ) -> some View {
        HStack(spacing: ThemeSpace.group) {
            lead
            LabeledContent {
                HStack(spacing: ThemeSpace.group) {
                    if let value, !value.isEmpty {
                        // A value stays on one line, as the boards keep
                        // it, and the name beside it wraps instead.
                        Text(verbatim: value)
                            .foregroundStyle(secondaryColor)
                            .fixedSize(horizontal: true, vertical: false)
                    }
                    content
                    if chevron {
                        Image(systemName: "chevron.forward")
                            .themeIcon(.small)
                            .foregroundStyle(Theme.macTextSecondary)
                            .accessibilityHidden(true)
                    }
                }
            } label: {
                rowLabel(action: labelAction)
                    .accessibilitySortPriority(1)
            }
        }
        .contentShape(Rectangle())
    }

    @ViewBuilder
    private func rowLabel(action: (() -> Void)?) -> some View {
        if let action {
            Button(action: action) { rowWords.contentShape(Rectangle()) }
                .buttonStyle(.accentPlain)
        }
        else {
            rowWords
        }
    }

    private var rowWords: some View {
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
                            ?? AnyShapeStyle(secondaryColor)
                    )
            }
        }
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
