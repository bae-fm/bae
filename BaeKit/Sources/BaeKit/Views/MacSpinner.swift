import SwiftUI

/// A wait with no length and no steps of its own, such as a list still
/// filling: the small spinner, with the words that say what is happening
/// beside it when there are any. A wait whose length is known, or a step of
/// a flow, draws a bar instead.
public struct MacSpinner: View {
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

    /// The spinner, named by the words beside it when it has them.
    private var spinner: some View {
        SystemSpinner()
            .accessibilityLabel(
                words ?? Text("Loading", tableName: "Controls", bundle: .module)
            )
    }

    public var body: some View {
        HStack(spacing: ThemeSpace.related) {
            spinner
            if let words {
                words
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

#if canImport(AppKit)
    /// macOS's own small spinner, which AppKit draws and names a busy
    /// indicator with no value. SwiftUI's indeterminate progress view draws
    /// its own and reports a progress of 0.
    private struct SystemSpinner: NSViewRepresentable {
        func makeNSView(context: Context) -> NSProgressIndicator {
            let indicator = NSProgressIndicator()
            indicator.style = .spinning
            indicator.isIndeterminate = true
            indicator.controlSize = .small
            indicator.startAnimation(nil)
            return indicator
        }

        func updateNSView(_ indicator: NSProgressIndicator, context: Context) {}

        func sizeThatFits(
            _ proposal: ProposedViewSize,
            nsView indicator: NSProgressIndicator,
            context: Context
        ) -> CGSize? {
            indicator.intrinsicContentSize
        }
    }
#else
    private struct SystemSpinner: View {
        var body: some View {
            ProgressView().controlSize(.small)
        }
    }
#endif
