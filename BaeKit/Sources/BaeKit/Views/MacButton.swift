import SwiftUI

/// How a `MacButton` is drawn.
public enum MacButtonKind: Sendable {
    /// The action a screen is for: the accent's button fill under white text.
    case primary
    /// Any other action: the control fill under the primary text colour.
    case secondary
    /// A secondary button with the height and corners of the field beside it.
    case fieldAction
    /// Words with no fill, at the end of a row or beside other text.
    case borderless
    /// Only its label, which the caller draws in its own look (a glyph, a
    /// heading with a chevron).
    case plain
    /// A glyph alone: secondary text in a box as high as a regular button.
    case icon
}

/// A macOS push button: `primary` for the action a screen is for, `secondary`
/// otherwise, `borderless` for words with no fill. A button with a `menu`
/// opens bae's menu under itself instead of acting.
public struct MacButton<Label: View>: View {
    private let kind: MacButtonKind
    private let press: Press
    private let label: Label
    private var tone = Tone.inherited
    private var isTinted = false
    private var controlSize: ControlSize?
    private var isFilling = false

    /// What a press does.
    private enum Press {
        case action(() -> Void)
        #if canImport(AppKit)
            case menu(() -> [MacMenuEntry], isOpen: Binding<Bool>?)
        #endif
    }

    public init(
        kind: MacButtonKind = .secondary,
        action: @escaping () -> Void,
        @ViewBuilder label: () -> Label
    ) {
        self.kind = kind
        self.press = .action(action)
        self.label = label()
    }

    /// Words in the primary text colour, for a borderless button beside a
    /// step's default button.
    public func neutral() -> MacButton {
        var copy = self
        copy.tone = .neutral
        return copy
    }

    /// Words in the accent, for a borderless button among quiet words that
    /// would otherwise take their colour: Disconnect at a receiver row's end.
    public func accent() -> MacButton {
        var copy = self
        copy.tone = .accent
        return copy
    }

    /// A secondary button on a filled notice: its fill is the primary text
    /// colour at 12%, so it reads on a notice of every tone.
    public func tinted(_ isTinted: Bool = true) -> MacButton {
        var copy = self
        copy.isTinted = isTinted
        return copy
    }

    /// Words in the danger colour, for a button that removes or discards.
    public func destructive() -> MacButton {
        var copy = self
        copy.tone = .destructive
        return copy
    }

    /// Words in secondary text, for a button under quiet words: a failed
    /// view's centred notice. `onStage` takes the image viewer's stage text.
    public func muted(_ isMuted: Bool = true, onStage: Bool = false)
        -> MacButton
    {
        var copy = self
        copy.tone = isMuted ? .muted(onStage: onStage) : .inherited
        return copy
    }

    /// A button as wide as the space it is given, its words centred, instead
    /// of as wide as its words: the welcome window's three choices share one
    /// column.
    public func fillsWidth() -> MacButton {
        var copy = self
        copy.isFilling = true
        return copy
    }

    /// A small button.
    public func small() -> MacButton {
        var copy = self
        copy.controlSize = .small
        return copy
    }

    public var body: some View {
        sized
            .modifier(MacButtonSize(size: controlSize))
    }

    /// A button with words keeps them whole on one line at their own width,
    /// as the board's does, so a row beside it wraps instead of squeezing the
    /// button; one that `fillsWidth` takes the width it is given.
    @ViewBuilder
    private var sized: some View {
        switch kind {
        case .primary, .secondary, .fieldAction, .borderless:
            drawn.lineLimit(1)
                .fixedSize(horizontal: !isFilling, vertical: false)
        case .plain, .icon:
            drawn
        }
    }

    /// The label in the colour the tone sets. The colour goes on the label,
    /// because the kind's style sets its own colour around it.
    private var words: some View {
        iconOrLabel.modifier(MacButtonLook(tone: tone))
    }

    @ViewBuilder
    private var iconOrLabel: some View {
        if kind == .icon {
            label.themeIcon(.small)
                .foregroundStyle(Theme.textSecondary)
                .frame(width: 24, height: 24)
                .contentShape(Rectangle())
                .accessibilityRemoveTraits(.isSelected)
        }
        else {
            label
        }
    }

    @ViewBuilder
    private var drawn: some View {
        if case .action(let action) = press {
            Button(action: action) { words }
                .macButtonStyle(kind, tinted: isTinted)
        }
        #if canImport(AppKit)
            if case .menu(let content, let isOpen) = press {
                MacMenuOpener(
                    kind: kind,
                    tinted: isTinted,
                    content: content,
                    isOpen: isOpen,
                    label: words
                )
            }
        #endif
    }
}

/// The colour of a button's words.
private enum Tone {
    /// The kind's own colour, as what the button sits in sets it.
    case inherited
    /// The primary text colour.
    case neutral
    /// The accent.
    case accent
    /// The danger colour.
    case destructive
    /// Secondary text, or a stage's secondary text `onStage`.
    case muted(onStage: Bool)
}

private struct MacButtonLook: ViewModifier {
    let tone: Tone

    func body(content: Content) -> some View {
        switch tone {
        case .inherited:
            content
        case .neutral:
            content.foregroundStyle(Theme.textPrimary)
        case .accent:
            content.foregroundStyle(Theme.accent)
        case .destructive:
            content.foregroundStyle(Theme.danger)
        case .muted(let onStage):
            let color = onStage ? Theme.onFillSecondary : Theme.textSecondary
            content.foregroundStyle(color)
        }
    }
}

private struct MacButtonSize: ViewModifier {
    let size: ControlSize?

    func body(content: Content) -> some View {
        if let size {
            content.controlSize(size)
        }
        else {
            content
        }
    }
}

extension View {
    /// This button drawn as `kind` says.
    @ViewBuilder
    public func macButtonStyle(_ kind: MacButtonKind, tinted: Bool = false)
        -> some View
    {
        switch kind {
        case .primary:
            buttonStyle(PrimaryButtonStyle().accentFocus())
        case .secondary:
            buttonStyle(SecondaryButtonStyle(tinted: tinted).accentFocus())
        case .fieldAction:
            buttonStyle(
                SecondaryButtonStyle(
                    cornerRadius: ThemeRadius.field,
                    height: ThemeSize.field,
                    tinted: tinted
                )
                .accentFocus(cornerRadius: ThemeRadius.field)
            )
        case .borderless:
            buttonStyle(.macBorderless)
        case .plain, .icon:
            buttonStyle(.accentPlain)
        }
    }
}

extension MacButton where Label == Text {
    public init(
        _ title: LocalizedStringKey,
        kind: MacButtonKind = .secondary,
        action: @escaping () -> Void
    ) {
        self.init(kind: kind, action: action) { Text(title) }
    }

    /// A button whose words are already localized.
    public init<S: StringProtocol>(
        verbatim title: S,
        kind: MacButtonKind = .secondary,
        action: @escaping () -> Void
    ) {
        self.init(kind: kind, action: action) { Text(title) }
    }
}

#if canImport(AppKit)
    extension MacButton {
        /// A button that opens bae's menu of `menu` under itself, with
        /// `isOpen` true while the menu is open.
        public init(
            kind: MacButtonKind = .secondary,
            isOpen: Binding<Bool>? = nil,
            @MacMenuBuilder menu: @escaping () -> [MacMenuEntry],
            @ViewBuilder label: () -> Label
        ) {
            self.kind = kind
            self.press = .menu(menu, isOpen: isOpen)
            self.label = label()
        }
    }
#endif
