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
                    .foregroundStyle(Theme.macTextSecondary)
            }
        }
    }
}

#if canImport(AppKit)
    extension MacSpinner {
        final class NativeIndicator: NSProgressIndicator {
            private(set) var spinning = false

            override func accessibilityValue() -> NSNumber? { nil }

            /// Spins unless the transaction disables animations, as the shot
            /// renderer's does, so a capture draws the same still frame each
            /// time. The stopped spinner stays drawn.
            func applyAnimationPolicy(_ transaction: Transaction) {
                let spinning = !transaction.disablesAnimations
                guard self.spinning != spinning else { return }
                self.spinning = spinning
                if spinning {
                    startAnimation(nil)
                }
                else {
                    stopAnimation(nil)
                }
            }
        }
    }

    /// macOS's own small spinner, which AppKit draws. A wait of no known
    /// length has no amount done, so VoiceOver reads no value: SwiftUI's
    /// indeterminate progress view reports 0, and AppKit's spinner 1.
    private struct SystemSpinner: NSViewRepresentable {
        func makeNSView(context: Context) -> MacSpinner.NativeIndicator {
            let indicator = MacSpinner.NativeIndicator()
            indicator.style = .spinning
            indicator.isIndeterminate = true
            indicator.controlSize = .small
            indicator.isDisplayedWhenStopped = true
            indicator.applyAnimationPolicy(context.transaction)
            return indicator
        }

        func updateNSView(
            _ indicator: MacSpinner.NativeIndicator,
            context: Context
        ) {
            indicator.applyAnimationPolicy(context.transaction)
        }

        func sizeThatFits(
            _ proposal: ProposedViewSize,
            nsView indicator: MacSpinner.NativeIndicator,
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
