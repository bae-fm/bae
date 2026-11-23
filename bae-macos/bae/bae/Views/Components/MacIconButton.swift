import BaeKit
import SwiftUI

/// A glyph button with its label for VoiceOver and its help for the pointer.
struct MacIconButton: View {
    enum Look {
        case hover(active: Bool, faint: Bool, hitSize: CGFloat)
        case boxed
        case borderless
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
        iconSize: ThemeIcon = .large,
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
        let button = Button(role: role, action: action) {
            Image(systemName: symbol)
                .themeIcon(iconSize)
                .frame(width: hitSize, height: hitSize)
                .contentShape(Rectangle())
        }
        Group {
            switch look {
            case .hover(let active, let faint, _):
                button.buttonStyle(
                    IconHoverButtonStyle(isOn: active, faint: faint)
                        .accentFocus()
                )
            case .boxed:
                button.buttonStyle(BoxedIconButtonStyle(size: .small).focused)
            case .borderless:
                button.buttonStyle(.accentBorderless)
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
