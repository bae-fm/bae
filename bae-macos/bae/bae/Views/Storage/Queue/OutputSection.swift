import BaeKit
import SwiftUI

/// Pane showing the in-memory export queue: a header with the summary and
/// pause/retry controls, and a per-release row list. Hidden when the queue is
/// idle. Reads `OutputStore` at the leaf; the export projection is the sole
/// writer, so actions don't optimistically mutate — the output subscription
/// delivers each queue change. Mirrors
/// `DownloadsSection`.
struct OutputSection: View {
    @Environment(OutputStore.self)
    private var outputStore
    @Environment(Outputs.self)
    private var outputs

    var body: some View {
        let snapshot = outputStore.snapshot
        if !snapshot.outputs.isEmpty {
            Divider()
            VStack(spacing: 0) {
                QueueSectionHeader(
                    icon: "square.and.arrow.up",
                    title: "Export & Save",
                    paused: snapshot.paused,
                    summaryText: snapshot.summaryText,
                    retryDisabled: snapshot.total.failed == 0,
                    onSetPaused: { outputs.setOutputsPaused($0) },
                    onRetry: { outputs.retryOutputs() }
                )
                Divider()
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(snapshot.outputs, id: \.releaseId) { op in
                            OutputRow(op: op) {
                                outputs.cancelOutput(op.releaseId)
                            }
                            Divider()
                        }
                    }
                }
                .frame(maxHeight: 180)
            }
        }
    }

}

#if DEBUG
    #Preview("Populated") {
        OutputSection()
            .environment(PreviewData.outputStore())
            .environment(Outputs.stub())
            .frame(width: 700)
    }

    #Preview("Paused") {
        OutputSection()
            .environment(
                PreviewData.outputStore(
                    PreviewData.outputSnapshot(paused: true)
                )
            )
            .environment(Outputs.stub())
            .frame(width: 700)
    }
#endif
