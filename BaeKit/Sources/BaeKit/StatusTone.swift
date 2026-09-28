import SwiftUI

/// What a chip or notice says about its subject, and the colour that says it.
public enum StatusTone: Sendable {
    case neutral
    case accent
    case info
    case success
    case warning
    case danger
    case activity

    /// The text, glyph and outline colour.
    public var color: Color {
        switch self {
        case .neutral: .secondary
        case .accent: Theme.accent
        case .info: Theme.info
        case .success: Theme.success
        case .warning: Theme.warning
        case .danger: Theme.danger
        case .activity: Theme.activity
        }
    }

    /// The fill behind the tone's own text.
    public var fill: Color {
        color.opacity(ThemeOpacity.tint)
    }

    /// The glyph a notice in this tone leads with.
    public var symbol: String? {
        switch self {
        case .info: "info.circle.fill"
        case .success: "checkmark.circle.fill"
        case .warning: "exclamationmark.triangle.fill"
        case .danger: "xmark.octagon.fill"
        case .neutral, .accent, .activity: nil
        }
    }
}

extension View {
    /// Pads, fills and rounds the content as a notice in its tone.
    public func notice(_ tone: StatusTone) -> some View {
        padding(ThemeSpace.group)
            .background(
                tone.fill,
                in: RoundedRectangle(cornerRadius: ThemeRadius.control)
            )
    }

    /// Sets the content on a card: the surface, rounded, with a hairline edge;
    /// an elevated card floats over other content with a shadow.
    public func card(elevated: Bool = false) -> some View {
        background(
            elevated ? Theme.surfaceElevated : Theme.surface,
            in: RoundedRectangle(cornerRadius: ThemeRadius.card)
        )
        .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.card))
        .overlay {
            RoundedRectangle(cornerRadius: ThemeRadius.card)
                .strokeBorder(Theme.hairline, lineWidth: 1)
        }
        .shadow(
            color: elevated ? Theme.shadow : .clear,
            radius: ThemeSpace.group,
            y: ThemeSpace.compact
        )
    }
}

/// A short label on a tinted fill: a status, a count, a tag or a role. A chip
/// that can be taken away ends with an ✕ that does it.
public struct StatusChip: View {
    /// The ✕ at the chip's end: what pressing it says it does, and doing it.
    public struct Removal {
        let label: Text
        let action: () -> Void

        public init(_ label: Text, action: @escaping () -> Void) {
            self.label = label
            self.action = action
        }
    }

    private let label: Text
    private let tone: StatusTone
    private let symbol: String?
    private let removal: Removal?

    public init(
        _ label: Text,
        tone: StatusTone = .neutral,
        symbol: String? = nil,
        removal: Removal? = nil
    ) {
        self.label = label
        self.tone = tone
        self.symbol = symbol
        self.removal = removal
    }

    public init(
        _ key: LocalizedStringKey,
        tone: StatusTone = .neutral,
        symbol: String? = nil
    ) {
        self.init(Text(key), tone: tone, symbol: symbol)
    }

    public init(
        verbatim string: String,
        tone: StatusTone = .neutral,
        symbol: String? = nil,
        removal: Removal? = nil
    ) {
        self.init(
            Text(verbatim: string),
            tone: tone,
            symbol: symbol,
            removal: removal
        )
    }

    public var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            if let symbol {
                Image(systemName: symbol)
                    .themeIcon(.badge)
            }
            label
            if let removal {
                Button(action: removal.action) {
                    Image(systemName: "xmark")
                        .themeIcon(.badge)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(removal.label)
                .help(removal.label)
            }
        }
        .themeText(.chip)
        .lineLimit(1)
        .foregroundStyle(tone.color)
        .padding(.horizontal, ThemeSpace.compact)
        .padding(.vertical, ThemeSpace.line)
        .background(
            tone.fill,
            in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
        )
    }
}

/// A line saying something failed.
public struct ErrorText: View {
    private let text: Text

    /// A message already in the person's language; a literal takes the
    /// localized initializer instead.
    @_disfavoredOverload
    public init(_ message: String) {
        text = Text(verbatim: message)
    }

    public init(_ key: LocalizedStringKey) {
        text = Text(key)
    }

    public var body: some View {
        text
            .themeText(.body)
            .foregroundStyle(Theme.danger)
    }
}
