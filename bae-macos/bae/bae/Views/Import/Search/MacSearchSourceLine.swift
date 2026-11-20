import BaeKit
import SwiftUI

/// One catalog that a typed search still waits on, set in from the results'
/// edge: a spinner while it searches, a warning with its Retry once it failed.
struct MacSearchSourceLine: View {
    enum Status {
        case searching
        /// The catalog did not answer; `reason` shows on hover.
        case failed(reason: String)
    }

    let catalog: String
    let status: Status
    let onRetry: () -> Void

    var body: some View {
        Group {
            switch status {
            case .searching:
                HStack(spacing: ThemeSpace.compact) {
                    MacSpinner()
                        .frame(
                            width: ThemeIcon.small.size,
                            height: ThemeIcon.small.size
                        )
                    Text(
                        "Searching \(catalog)…",
                        comment:
                            "Find online, under a typed search's results: a catalog still searching; the argument is the catalog's name."
                    )
                    .foregroundStyle(.tertiary)
                }
            case .failed(let reason):
                HStack(spacing: ThemeSpace.compact) {
                    StatusGlyph(.warning)
                        .accessibilityLabel("Failed")
                    Text(catalog)
                        .foregroundStyle(.tertiary)
                        .help(reason)
                    MacButton("Retry", kind: .borderless, action: onRetry)
                }
            }
        }
        .themeText(.detail)
        .foregroundStyle(.secondary)
        .padding(.leading, ThemeSpace.group)
    }
}
