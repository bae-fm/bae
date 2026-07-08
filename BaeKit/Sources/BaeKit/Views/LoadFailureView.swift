import SwiftUI

/// A centered load-failure placeholder: the error line plus a Retry button.
/// The shared shape every full-view load surface uses when a fetch fails —
/// album detail, the library and composer grids, the storage table — so a
/// failed load reads as an error the user can retry, never as an empty result
/// or an endless spinner. `line` is already-localized prose (shown verbatim).
public struct LoadFailureView: View {
    private let line: String
    private let onRetry: () -> Void

    public init(line: String, onRetry: @escaping () -> Void) {
        self.line = line
        self.onRetry = onRetry
    }

    public var body: some View {
        VStack(spacing: 12) {
            Text(line)
                .font(.callout)
                .foregroundStyle(.red)
                .multilineTextAlignment(.center)
            Button("Retry", action: onRetry)
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
