import SwiftUI

/// How a `MacButton` is drawn.
public enum MacButtonKind: Sendable {
    /// The action a screen is for: the accent's button fill under white text.
    case primary
    /// Any other action: the control fill under the primary text colour.
    case secondary
    /// Words with no fill, at the end of a row or beside other text.
    case borderless
    /// Only its label, which the caller draws in its own look (a glyph, a
    /// heading with a chevron).
    case plain
}

/// A macOS push button: `primary` for the action a screen is for, `secondary`
/// otherwise, `borderless` for words with no fill. A button with a `menu`
/// opens bae's menu under itself instead of acting.
public struct MacButton<Label: View>: View {
    private let kind: MacButtonKind
    private let press: Press
    private let label: Label
    private var tone = Tone.inherited
    private var controlSize: ControlSize?

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

    /// Words in secondary text, for a button under quiet words: a failed
    /// view's centred notice. `onStage` takes the image viewer's stage text.
    public func muted(_ isMuted: Bool = true, onStage: Bool = false)
        -> MacButton
    {
        var copy = self
        copy.tone = isMuted ? .muted(onStage: onStage) : .inherited
        return copy
    }

    /// A small button.
    public func small() -> MacButton {
        var copy = self
        copy.controlSize = .small
        return copy
    }

    public var body: some View {
        drawn
            .modifier(MacButtonLook(tone: tone))
            .modifier(MacButtonSize(size: controlSize))
    }

    @ViewBuilder
    private var drawn: some View {
        if case .action(let action) = press {
            Button(action: action) { label }
                .macButtonStyle(kind)
        }
        #if canImport(AppKit)
            if case .menu(let content, let isOpen) = press {
                MacMenuOpener(
                    kind: kind,
                    content: content,
                    isOpen: isOpen,
                    label: label
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
            content.foregroundStyle(Theme.textPrimary).tint(Theme.textPrimary)
        case .accent:
            content.foregroundStyle(Theme.accent).tint(Theme.accent)
        case .muted(let onStage):
            let color = onStage ? Theme.onFillSecondary : Color.secondary
            content.foregroundStyle(color).tint(color)
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
    func macButtonStyle(_ kind: MacButtonKind) -> some View {
        switch kind {
        case .primary:
            buttonStyle(PrimaryButtonStyle())
        case .secondary:
            buttonStyle(SecondaryButtonStyle().accentFocus())
        case .borderless:
            buttonStyle(.accentBorderless)
        case .plain:
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
