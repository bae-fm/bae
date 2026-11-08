import SwiftUI

/// What a chip or notice says about its subject, and the colour that says it.
public enum StatusTone: Sendable, Equatable {
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
        case .warning: "exclamationmark.circle.fill"
        case .danger: "exclamationmark.circle.fill"
        case .neutral, .accent, .activity: nil
        }
    }
}

/// A tone's glyph in the tone's colour; nothing for a tone with no glyph.
/// Danger is its colour mixed 65% with secondary text: still red, calmer
/// beside a warning's amber.
public struct StatusGlyph: View {
    private let tone: StatusTone
    @Environment(\.self)
    private var environment

    public init(_ tone: StatusTone) {
        self.tone = tone
    }

    public var body: some View {
        if let symbol = tone.symbol {
            Image(systemName: symbol)
                .foregroundStyle(color)
        }
    }

    private var color: Color {
        guard tone == .danger else { return tone.color }
        let danger = Theme.danger.resolve(in: environment)
        let grey = Color.secondary.resolve(in: environment)
        let share: Float = 0.65
        return Color(
            .sRGBLinear,
            red: Double(
                danger.linearRed * share + grey.linearRed * (1 - share)
            ),
            green: Double(
                danger.linearGreen * share + grey.linearGreen * (1 - share)
            ),
            blue: Double(
                danger.linearBlue * share + grey.linearBlue * (1 - share)
            ),
            opacity: Double(danger.opacity * share + grey.opacity * (1 - share))
        )
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

/// A short label on a tinted fill: a status, a count, a tag or a role.
public struct StatusChip: View {
    private let label: Text
    private let tone: StatusTone
    private let symbol: String?

    public init(
        _ label: Text,
        tone: StatusTone = .neutral,
        symbol: String? = nil
    ) {
        self.label = label
        self.tone = tone
        self.symbol = symbol
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
        symbol: String? = nil
    ) {
        self.init(Text(verbatim: string), tone: tone, symbol: symbol)
    }

    public var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            if let symbol {
                Image(systemName: symbol)
                    .themeIcon(.badge)
            }
            label
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
