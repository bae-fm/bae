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
