import BaeKit
import SwiftUI

enum StableOptionalTextForeground {
    case primary
    case secondary
    case tertiary
}

struct StableOptionalText: View {
    let text: String?
    let font: Font
    let foreground: StableOptionalTextForeground
    let lineHeight: CGFloat
    let lineLimit: Int?

    init(
        text: String?,
        font: Font,
        foreground: StableOptionalTextForeground = .primary,
        lineHeight: CGFloat,
        lineLimit: Int? = nil
    ) {
        self.text = text
        self.font = font
        self.foreground = foreground
        self.lineHeight = lineHeight
        self.lineLimit = lineLimit
    }

    var body: some View {
        ZStack(alignment: .leading) {
            Color.clear
                .frame(height: lineHeight)
                .accessibilityHidden(true)
            styledText
                .opacity(hasText ? 1 : 0)
                .allowsHitTesting(hasText)
                .accessibilityHidden(!hasText)
        }
    }

    @ViewBuilder
    private var styledText: some View {
        switch foreground {
        case .primary:
            baseText
        case .secondary:
            baseText.foregroundStyle(.secondary)
        case .tertiary:
            baseText.foregroundStyle(.tertiary)
        }
    }

    private var baseText: some View {
        Text(displayText)
            .font(font)
            .lineLimit(lineLimit)
    }

    private var hasText: Bool {
        text != nil
    }

    private var displayText: String {
        switch text {
        case .some(let text):
            text
        case .none:
            " "
        }
    }
}

#if DEBUG
    #Preview("Stable Optional Text") {
        // The nil row reserves the same line height as the present rows, so a
        // stack of these never shifts when a value appears or disappears.
        VStack(alignment: .leading, spacing: 8) {
            StableOptionalText(
                text: "Track Title",
                font: .system(size: 14, weight: .medium),
                lineHeight: 18
            )
            StableOptionalText(
                text: nil,
                font: .system(size: 14, weight: .medium),
                foreground: .secondary,
                lineHeight: 18
            )
            StableOptionalText(
                text: "Artist Name",
                font: .system(size: 12),
                foreground: .tertiary,
                lineHeight: 16
            )
        }
        .padding(24)
        .frame(width: 240, alignment: .leading)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
