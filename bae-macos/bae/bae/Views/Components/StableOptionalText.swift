import BaeKit
import SwiftUI

enum StableOptionalTextForeground {
    case primary
    case secondary
    case tertiary
}

struct StableOptionalText: View {
    let text: String?
    let role: ThemeText
    let foreground: StableOptionalTextForeground
    let lineLimit: Int?

    init(
        text: String?,
        role: ThemeText,
        foreground: StableOptionalTextForeground = .primary,
        lineLimit: Int? = nil
    ) {
        self.text = text
        self.role = role
        self.foreground = foreground
        self.lineLimit = lineLimit
    }

    var body: some View {
        ZStack(alignment: .leading) {
            // One line of the role's text, so the view keeps its height with
            // no text.
            Text(verbatim: " ")
                .themeText(role)
                .lineLimit(1)
                .hidden()
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
            baseText.foregroundStyle(Theme.macTextSecondary)
        case .tertiary:
            baseText.foregroundStyle(Theme.textTertiary)
        }
    }

    private var baseText: some View {
        Text(displayText)
            .themeText(role)
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
