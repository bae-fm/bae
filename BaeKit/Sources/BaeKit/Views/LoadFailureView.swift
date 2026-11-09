import SwiftUI

/// A view that failed whole, drawn quietly with no box and no glyph, centred
/// in the view it replaces: the message in secondary text, then one row with
/// Show details and Retry. Open, the causes sit under the row in an inset
/// panel.
public struct LoadFailureView: View {
    private let error: DisplayError
    private let onRetry: () -> Void

    public init(error: DisplayError, onRetry: @escaping () -> Void) {
        self.error = error
        self.onRetry = onRetry
    }

    @State
    private var detailExpanded = false

    public var body: some View {
        VStack(spacing: ThemeSpace.related) {
            Text(message)
                .themeText(.heading)
                .fontWeight(.regular)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            HStack(spacing: ThemeSpace.group + ThemeSpace.line) {
                if error.detail != nil {
                    Button(detailExpanded ? "Hide details" : "Show details") {
                        detailExpanded.toggle()
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(Theme.accent)
                }
                Button("Retry", action: onRetry)
            }
            if detailExpanded, let detail = error.detail,
                let excerpt = error.detailExcerpt
            {
                ErrorCausesPanel(
                    detail: detail,
                    excerpt: excerpt
                )
                .frame(maxWidth: 340)
            }
        }
        .padding(ThemeSpace.page)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// The line without its closing period.
    private var message: String {
        error.line.hasSuffix(".") ? String(error.line.dropLast()) : error.line
    }
}
