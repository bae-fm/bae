import SwiftUI

/// A centered error line and Retry button for a full view whose load failed;
/// `line` is already localized.
public struct LoadFailureView: View {
    private let line: String
    private let onRetry: () -> Void

    public init(line: String, onRetry: @escaping () -> Void) {
        self.line = line
        self.onRetry = onRetry
    }

    public var body: some View {
        VStack(spacing: ThemeSpace.group) {
            Text(line)
                .themeText(.body)
                .foregroundStyle(Theme.danger)
                .multilineTextAlignment(.center)
            Button("Retry", action: onRetry)
        }
        .padding(ThemeSpace.page)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
