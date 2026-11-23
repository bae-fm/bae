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

private struct MacRowNote {
    let text: Text
    let tone: StatusTone?
}

private struct MacRowOptions {
    var value: String?
    var note: MacRowNote?
    var chip: String?
    var isStrong = false
    var press: MacRowPress?
    var selection: Bool?
}

/// A settings row, as the design system's `MacRow` draws one: a lead such as
/// a person's picture, the row's name with a note under it, then its value in
/// secondary text and its controls at the right, a group's space apart.
/// Inside a settings box the row is one line of the box.
public struct MacRow<Lead: View, Label: View, Content: View, Info: View>: View {
    /// What pressing the row does, which also says how it reads.
    public typealias Press = MacRowPress

    private let lead: Lead
    private let label: Label
    private let content: Content
    private let info: Info
    private var options: MacRowOptions

    @Environment(\.macRowPlacement)
    private var placement
    @Environment(\.macRowIsNested)
    private var isNested
    @Environment(\.isEnabled)
    private var isEnabled

    private init(
        lead: Lead,
        label: Label,
        content: Content,
        info: Info,
        options: MacRowOptions
    ) {
        self.lead = lead
        self.label = label
        self.content = content
        self.info = info
        self.options = options
    }

    /// The row with `lead` before its name, such as a person's picture.
    public func lead<NewLead: View>(
        @ViewBuilder _ lead: () -> NewLead
    ) -> MacRow<NewLead, Label, Content, Info> where Lead == EmptyView {
        MacRow<NewLead, Label, Content, Info>(
            lead: lead(),
            label: label,
            content: content,
            info: info,
            options: options
        )
    }

    /// Places an info button beside the row's name.
    public func info<NewInfo: View>(
        @ViewBuilder _ info: () -> NewInfo
    ) -> MacRow<Lead, Label, Content, NewInfo> where Info == EmptyView {
        MacRow<Lead, Label, Content, NewInfo>(
            lead: lead,
            label: label,
            content: content,
            info: info(),
            options: options
        )
    }

    /// The row's value, in secondary text before its controls; nil or empty
    /// draws none.
    public func value(_ words: String?) -> MacRow {
        var copy = self
        copy.options.value = words
        return copy
    }

    /// A line under the row's name, in secondary text, or in `tone`'s colour
    /// when it says something in that tone, such as why an invite failed.
    public func note(_ text: Text?, tone: StatusTone? = nil) -> MacRow {
        var copy = self
        copy.options.note = text.map { MacRowNote(text: $0, tone: tone) }
        return copy
    }

    /// A chip after the row's name, such as a person's role.
    public func chip(_ words: String?) -> MacRow {
        var copy = self
        copy.options.chip = words
        return copy
    }

    /// The row's name in the weight of a name that heads a row, such as a
    /// person's.
    public func strong(_ on: Bool = true) -> MacRow {
        var copy = self
        copy.options.isStrong = on
        return copy
    }

    /// The row as a button that does `press`.
    public func pressing(_ press: Press?) -> MacRow {
        var copy = self
        copy.options.press = press
        return copy
    }

    /// Draws a choice row, filling it when selected.
    public func selected(_ selected: Bool) -> MacRow {
        var copy = self
        copy.options.selection = selected
        return copy
    }

    public var body: some View {
        rowAppearance
    }

    private var textOpacity: Double {
        if isEnabled { return 1 }
        return isNested ? 0.45 : ThemeOpacity.off
    }

    @ViewBuilder
    private var rowAppearance: some View {
        if let selection = options.selection {
            rowControl.macSelection(selection, in: Rectangle())
        }
        else {
            rowControl
        }
    }

    @ViewBuilder
    private var rowControl: some View {
        switch options.press {
        case .opens(let action)?:
            Button(action: action) { rowWithPlacement(chevron: true) }
                .buttonStyle(.accentPlain)
                .placesItselfInRow()
        case .label(let chevron, let action)?:
            displayRow(chevron: chevron, labelAction: action)
        case .removes(let action)?:
            Button(role: .destructive, action: action) {
                rowWithPlacement(chevron: false).foregroundStyle(Theme.danger)
            }
            .buttonStyle(.accentPlain)
            .placesItselfInRow()
        case nil:
            displayRow(chevron: false)
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
        if options.selection != nil {
            rowWithPlacement(chevron: chevron, labelAction: labelAction)
                .placesItselfInRow()
        }
        else {
            // VoiceOver reads the name and note before the controls at the
            // row's end, even when a control is centred between two lines.
            row(chevron: chevron, labelAction: labelAction)
                .accessibilityElement(children: .contain)
        }
    }

    private var secondaryColor: Color {
        options.selection == true
            ? Theme.onFillSecondary : Theme.macTextSecondary
    }

    private func row(
        chevron: Bool,
        labelAction: (() -> Void)?
    ) -> some View {
        HStack(spacing: ThemeSpace.group) {
            lead
            LabeledContent {
                HStack(spacing: ThemeSpace.group) {
                    if let value = options.value, !value.isEmpty {
                        // A value stays on one line and gives up room to
                        // the controls beside it.
                        Text(verbatim: value)
                            .foregroundStyle(secondaryColor)
                            .opacity(textOpacity)
                            .lineLimit(1)
                    }
                    content.layoutPriority(1)
                    if chevron {
                        Image(systemName: "chevron.forward")
                            .themeIcon(.small)
                            .foregroundStyle(Theme.macTextSecondary)
                            .opacity(textOpacity)
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
                Group {
                    if options.isStrong {
                        label.themeText(.strong)
                    }
                    else {
                        label
                    }
                }
                .opacity(textOpacity)
                if let chip = options.chip {
                    StatusChip(verbatim: chip)
                }
                info
            }
            if let note = options.note {
                note.text
                    .themeText(.detail)
                    .foregroundStyle(
                        note.tone.map { AnyShapeStyle($0.color) }
                            ?? AnyShapeStyle(secondaryColor)
                    )
                    .opacity(textOpacity)
            }
        }
    }
}

extension MacRow where Lead == EmptyView, Info == EmptyView {
    public init(
        @ViewBuilder content: () -> Content,
        @ViewBuilder label: () -> Label
    ) {
        self.init(
            lead: EmptyView(),
            label: label(),
            content: content(),
            info: EmptyView(),
            options: MacRowOptions()
        )
    }
}

extension MacRow where Lead == EmptyView, Label == Text, Info == EmptyView {
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

extension MacRow
where Lead == EmptyView, Label == Text, Content == EmptyView, Info == EmptyView
{
    /// A row whose right side is `value` in secondary text.
    public init<S: StringProtocol>(_ title: LocalizedStringKey, value: S) {
        self.init(title, value: value) {}
    }
}

extension MacRow
where
    Lead == EmptyView,
    Label == Text,
    Content == ToneDotValue,
    Info == EmptyView
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
