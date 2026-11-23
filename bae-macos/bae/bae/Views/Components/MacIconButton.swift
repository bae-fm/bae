import BaeKit
import SwiftUI

/// A glyph button with its label for VoiceOver and its help for the pointer.
struct MacIconButton: View {
    enum Look {
        case hover(active: Bool, faint: Bool, hitSize: CGFloat)
        case boxed(small: Bool)
        case borderless
    }

    /// What pressing the button does.
    enum Press {
        case action(role: ButtonRole?, () -> Void)
        /// Opens bae's menu of the entries under the button.
        case menu(() -> [MacMenuEntry])
    }

    let symbol: String
    let label: LocalizedStringKey
    let help: LocalizedStringKey?
    let iconSize: ThemeIcon
    let look: Look
    let press: Press

    init(
        symbol: String,
        label: LocalizedStringKey,
        help: LocalizedStringKey? = nil,
        active: Bool = false,
        faint: Bool = false,
        iconSize: ThemeIcon = .large,
        hitSize: CGFloat = ThemeSize.hitTarget,
        role: ButtonRole? = nil,
        action: @escaping () -> Void
    ) {
        self.init(
            symbol: symbol,
            label: label,
            help: help,
            iconSize: iconSize,
            look: .hover(active: active, faint: faint, hitSize: hitSize),
            role: role,
            action: action
        )
    }

    init(
        symbol: String,
        label: LocalizedStringKey,
        help: LocalizedStringKey? = nil,
        iconSize: ThemeIcon,
        look: Look,
        role: ButtonRole? = nil,
        action: @escaping () -> Void
    ) {
        self.init(
            symbol: symbol,
            label: label,
            help: help,
            iconSize: iconSize,
            look: look,
            press: .action(role: role, action)
        )
    }

    private init(
        symbol: String,
        label: LocalizedStringKey,
        help: LocalizedStringKey?,
        iconSize: ThemeIcon,
        look: Look,
        press: Press
    ) {
        self.symbol = symbol
        self.label = label
        self.help = help
        self.iconSize = iconSize
        self.look = look
        self.press = press
    }

    /// A button that opens a menu: the glyph in secondary text, as the
    /// resting icon button draws it.
    static func menu(
        symbol: String,
        label: LocalizedStringKey,
        help: LocalizedStringKey? = nil,
        iconSize: ThemeIcon = .large,
        @MacMenuBuilder items: @escaping () -> [MacMenuEntry]
    ) -> Self {
        Self(
            symbol: symbol,
            label: label,
            help: help,
            iconSize: iconSize,
            look: .hover(
                active: false,
                faint: false,
                hitSize: ThemeSize.hitTarget
            ),
            press: .menu(items)
        )
    }

    static func remove(_ name: String, action: @escaping () -> Void) -> Self {
        Self(
            symbol: "xmark.circle.fill",
            label: "Remove \(name)",
            faint: true,
            iconSize: .medium,
            hitSize: BoxedIconButtonStyle.Size.small.side,
            role: .destructive,
            action: action
        )
    }

    var body: some View {
        switch press {
        case .action(let role, let action):
            actionButton(role: role, action: action)
        case .menu(let items):
            MacButton(kind: .plain, menu: items) {
                glyph.foregroundStyle(Theme.macTextSecondary)
            }
            .help(help ?? label)
            .accessibilityLabel(label)
        }
    }

    private var glyph: some View {
        Image(systemName: symbol)
            .themeIcon(iconSize)
            .frame(width: hitSize, height: hitSize)
            .contentShape(Rectangle())
    }

    private func actionButton(
        role: ButtonRole?,
        action: @escaping () -> Void
    ) -> some View {
        let button = Button(role: role, action: action) {
            glyph
        }
        return Group {
            switch look {
            case .hover(let active, let faint, _):
                button.buttonStyle(
                    IconHoverButtonStyle(isOn: active, faint: faint)
                        .accentFocus()
                )
            case .boxed(let small):
                button.buttonStyle(
                    BoxedIconButtonStyle(size: small ? .small : .control)
                        .focused
                )
            case .borderless:
                button
                    .foregroundStyle(Theme.macTextSecondary)
                    .buttonStyle(.accentBorderless)
            }
        }
        .help(help ?? label)
        .accessibilityLabel(label)
    }

    private var hitSize: CGFloat? {
        if case .hover(_, _, let hitSize) = look {
            hitSize
        }
        else {
            nil
        }
    }
}
