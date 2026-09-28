import SwiftUI

/// The failure and a Retry button for a full view whose load failed.
public struct LoadFailureView: View {
    private let error: DisplayError
    private let onRetry: () -> Void

    public init(error: DisplayError, onRetry: @escaping () -> Void) {
        self.error = error
        self.onRetry = onRetry
    }

    public var body: some View {
        VStack(spacing: ThemeSpace.group) {
            ErrorDetailDisclosure(error: error)
                .fixedSize(horizontal: false, vertical: true)
            Button("Retry", action: onRetry)
        }
        .padding(ThemeSpace.page)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
