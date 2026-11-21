import BaeKit
import SwiftUI

/// One transfer line: its words, at the end how many of its releases are
/// done or what holds its queue back, then the button that pauses its queue,
/// and under them a bar of its batch's bytes; a line with no bar of bytes
/// shows the moving bar.
struct MacTransferLine: View {
    /// A queue a line pauses, and whether it is paused now.
    struct Queue {
        let kind: StorageQueueKind
        let paused: Bool

        init(_ kind: StorageQueueKind, paused: Bool) {
            self.kind = kind
            self.paused = paused
        }
    }

    let title: String
    let count: String?
    let queue: Queue?
    let bar: LatestSource<Double>?

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            HStack(alignment: .center, spacing: ThemeSpace.related) {
                Text(title)
                Spacer(minLength: 0)
                HStack(spacing: ThemeSpace.compact) {
                    if let count {
                        Text(count)
                            .foregroundStyle(Theme.macTextSecondary)
                            .monospacedDigit()
                    }
                    if let queue {
                        QueuePauseButton(
                            queue: queue.kind,
                            paused: queue.paused,
                            glyph: .medium,
                            queueNamed: true
                        )
                        .buttonStyle(
                            BoxedIconButtonStyle(size: .small).accentFocus()
                        )
                    }
                }
            }
            if let bar {
                ShareBar(share: bar)
                    .accessibilityLabel(title)
            }
            else {
                ProgressTrackBar(progress: nil)
                    .accessibilityLabel(title)
            }
        }
        .padding(.top, ThemeSpace.group)
        .padding(.bottom, ThemeSpace.group + ThemeSpace.inline)
    }
}
