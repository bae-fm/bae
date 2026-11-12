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

    /// The glyph's colour: a tone with a glyph has its own icon token, since
    /// a filled shape wants a different weight than text.
    public var iconColor: Color {
        switch self {
        case .info: Theme.infoIcon
        case .success: Theme.successIcon
        case .warning: Theme.warningIcon
        case .danger: Theme.dangerIcon
        case .neutral, .accent, .activity: color
        }
    }

    /// The fill of a filled notice: a toned notice has its own, a pale tone in
    /// light mode and the neutral fill in dark; the tones with no glyph keep
    /// their tint.
    public var noticeFill: Color {
        switch self {
        case .info: Theme.infoNotice
        case .success: Theme.successNotice
        case .warning: Theme.warningNotice
        case .danger: Theme.dangerNotice
        case .neutral, .accent, .activity: fill
        }
    }

    /// Whether a filled notice draws this tone's text in `Theme.onNotice`, as
    /// it has its own fill.
    var hasNoticeFill: Bool {
        symbol != nil
    }

    /// The fill behind the tone's own text.
    public var fill: Color {
        color.opacity(ThemeOpacity.tint)
    }

    /// The filled form of the tone's glyph, for a chip.
    public var symbol: String? {
        switch self {
        case .info: "info.circle.fill"
        case .success: "checkmark.circle.fill"
        case .warning: "exclamationmark.circle.fill"
        case .danger: "exclamationmark.circle.fill"
        case .neutral, .accent, .activity: nil
        }
    }

    /// The outline form of the glyph, which every notice draws.
    public var outlineSymbol: String? {
        switch self {
        case .info: "info.circle"
        case .success: "checkmark.circle"
        case .warning: "exclamationmark.circle"
        case .danger: "exclamationmark.circle"
        case .neutral, .accent, .activity: nil
        }
    }
}

/// How a notice sits in its place.
public enum NoticeLook: Sendable, Equatable {
    /// Padded and filled with the tone's notice colour, its text in
    /// `onNotice`.
    case filled
    /// No fill, in the screen's own text, with an outline glyph.
    case quiet
    /// A view that failed whole: in the middle of the space it is given, a
    /// grey outline glyph in front of a title in primary text, the buttons in
    /// secondary text in a row under it. No fill.
    case centered
    /// The centred layout on the image viewer's dark stage, in the stage's own
    /// text colours.
    case centeredOnStage
}

/// A tone's glyph in the tone's icon colour, filled or as an outline; nothing
/// for a tone with no glyph. The token carries the calmer danger: still red,
/// quieter beside a warning's amber.
public struct StatusGlyph: View {
    private let tone: StatusTone
    private let outline: Bool
    private let muted: Bool

    /// `muted` draws the glyph in the grey of the words beside it (secondary
    /// text), for a failure that should not call attention to itself.
    public init(
        _ tone: StatusTone,
        outline: Bool = false,
        muted: Bool = false
    ) {
        self.tone = tone
        self.outline = outline
        self.muted = muted
    }

    public var body: some View {
        if let symbol = outline ? tone.outlineSymbol : tone.symbol {
            Image(systemName: symbol)
                .foregroundStyle(color)
        }
    }

    /// The glyph carries the tone in both modes: its icon colour, or the
    /// grey of the words beside it when muted.
    private var color: Color {
        muted ? Color.secondary : tone.iconColor
    }
}

extension View {
    /// Pads the content as a notice in its tone, and in the filled look fills
    /// and rounds it; the quiet look has no fill and pads only above and
    /// below, the row it sits in insetting it at the sides.
    /// The centred looks have no fill and fill the space they are given: the
    /// view they replace, or 300 points where it has none, which no theme
    /// size gives. On the stage they take that space as it is.
    @ViewBuilder
    func notice(_ tone: StatusTone, look: NoticeLook = .filled)
        -> some View
    {
        switch look {
        case .filled:
            padding(ThemeSpace.group)
                .background(
                    tone.noticeFill,
                    in: RoundedRectangle(cornerRadius: ThemeRadius.control)
                )
                .foregroundStyle(
                    tone.hasNoticeFill ? Theme.onNotice : Color.primary
                )
        case .quiet:
            padding(.vertical, ThemeSpace.related + ThemeSpace.line)
        case .centered:
            padding(ThemeSpace.section)
                .frame(
                    maxWidth: .infinity,
                    minHeight: 300,
                    maxHeight: .infinity
                )
        case .centeredOnStage:
            frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    /// Sets the content on a card: the surface, rounded, with no edge, as the
    /// design system's box is; an elevated card floats over other content with
    /// a shadow.
    public func card(elevated: Bool = false) -> some View {
        background(
            elevated ? Theme.surfaceElevated : Theme.surface,
            in: RoundedRectangle(cornerRadius: ThemeRadius.card)
        )
        .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.card))
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
