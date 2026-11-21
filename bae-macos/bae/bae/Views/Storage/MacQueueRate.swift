import BaeKit
import SwiftUI

/// One running transfer queue: its glyph, how fast it goes, or how far
/// for exports, which count percent rather than bytes, or in the rate's place
/// what holds the queue back, and the button that pauses it, or resumes it
/// while paused.
struct MacQueueRate<Progress: View>: View {
    let queue: StorageQueueKind
    let paused: Bool
    /// What holds the queue back, in words.
    let hold: String?
    @ViewBuilder
    let progress: () -> Progress

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            HStack(spacing: ThemeSpace.inline) {
                glyph
                    .themeIcon(.small)
                if let hold {
                    Text(hold)
                }
                else {
                    progress()
                        .monospacedDigit()
                }
            }
            QueuePauseButton(queue: queue, paused: paused)
                .buttonStyle(.accentBorderless)
        }
        .lineLimit(1)
        .foregroundStyle(Theme.macTextSecondary)
        .themeText(.body)
        // A narrow table keeps each queue's rate whole on one line; the
        // release count and total give way instead.
        .fixedSize()
    }

    @ViewBuilder
    private var glyph: some View {
        switch queue {
        case .uploads:
            Image(systemName: "arrow.up")
                .foregroundStyle(Theme.accent)
                .accessibilityLabel(Text("Uploads"))
        case .downloads:
            Image(systemName: "arrow.down")
                .foregroundStyle(Theme.accent)
                .accessibilityLabel(Text("Downloads"))
        case .outputs:
            Image(systemName: "square.and.arrow.up")
                .accessibilityLabel(Text("Exports"))
        }
    }
}
