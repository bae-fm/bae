import BaeKit
import SwiftUI

/// A glyph button with its label for VoiceOver and its help for the pointer.
struct MacIconButton: View {
    enum Look {
        case hover(active: Bool, faint: Bool, hitSize: CGFloat)
        case boxed(small: Bool)
        case borderless
        /// Opens bae's menu of `items` under the button instead of acting.
        case menu(items: () -> [MacMenuEntry])
    }

    let symbol: String
    let label: LocalizedStringKey
    let help: LocalizedStringKey?
    let iconSize: ThemeIcon
    let look: Look
    let role: ButtonRole?
    let action: () -> Void

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
        self.symbol = symbol
        self.label = label
        self.help = help
        self.iconSize = iconSize
        self.look = look
        self.role = role
        self.action = action
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
        if case .menu(let items) = look {
            MacButton(kind: .plain, menu: items) {
                glyph
                    .foregroundStyle(Theme.macTextSecondary)
            }
            .help(help ?? label)
            .accessibilityLabel(label)
        }
        else {
            actionButton
        }
    }

    private var glyph: some View {
        Image(systemName: symbol)
            .themeIcon(iconSize)
            .frame(width: hitSize, height: hitSize)
            .contentShape(Rectangle())
    }

    @ViewBuilder
    private var actionButton: some View {
        let button = Button(role: role, action: action) {
            glyph
        }
        Group {
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
            case .menu:
                EmptyView()
            }
        }
        .help(help ?? label)
        .accessibilityLabel(label)
    }

    private var hitSize: CGFloat? {
        switch look {
        case .hover(_, _, let hitSize): hitSize
        case .menu: ThemeSize.hitTarget
        case .boxed, .borderless: nil
        }
    }
}
