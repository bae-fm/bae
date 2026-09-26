import SwiftUI

/// How much a notice asks of the person, and the colour that says it: a fact
/// worth knowing, something to act on before going further, or a failure.
/// Every tinted notice draws from here, so the three read the same wherever
/// they appear.
public enum NoticeTone: Sendable {
    case info
    case warning
    case error

    /// The icon, title and outline colour.
    public var tint: Color {
        switch self {
        case .info: .blue
        case .warning: .orange
        case .error: .red
        }
    }

    /// The notice's fill: the tint, faint enough for text to sit on it.
    public var fill: Color {
        tint.opacity(0.1)
    }
}

extension View {
    /// Fill and round a notice in its tone.
    public func noticeBackground(
        _ tone: NoticeTone,
        cornerRadius: CGFloat = 6
    ) -> some View {
        background(tone.fill)
            .clipShape(RoundedRectangle(cornerRadius: cornerRadius))
    }
}
