import SwiftUI

/// A wait with no length and no steps of its own, such as a list still
/// filling: the small spinner, with the words that say what is happening
/// beside it when there are any. A wait whose length is known, or a step of
/// a flow, draws a bar instead.
public struct WaitingIndicator: View {
    private let words: Text?

    /// The spinner alone, where what it waits on is said elsewhere.
    public init() {
        words = nil
    }

    /// The spinner, with `words` beside it.
    public init(_ words: Text) {
        self.words = words
    }

    /// The spinner, with the words `key` names beside it.
    public init(_ key: LocalizedStringKey) {
        words = Text(key)
    }

    public var body: some View {
        HStack(spacing: ThemeSpace.related) {
            ProgressView()
                .controlSize(.small)
            if let words {
                words
                    .themeText(.body)
                    .foregroundStyle(.secondary)
            }
        }
    }
}
