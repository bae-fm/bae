import SwiftUI

/// How much a notice asks of the person: a fact worth knowing, something to
/// act on before going further, or a failure.
public enum NoticeTone: Sendable {
    case info
    case warning
    case error

    /// The icon, title and outline colour.
    public var tint: Color {
        switch self {
        case .info: Theme.info
        case .warning: Theme.warning
        case .error: Theme.danger
        }
    }

    /// The notice's fill: the tint, faint enough for text to sit on it.
    public var fill: Color {
        tint.opacity(ThemeOpacity.tint)
    }
}

extension View {
    /// Fill and round a notice in its tone.
    public func noticeBackground(_ tone: NoticeTone) -> some View {
        background(tone.fill)
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.control))
    }
}
