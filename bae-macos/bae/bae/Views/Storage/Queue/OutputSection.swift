import BaeKit
import SwiftUI

/// Compact queue-wide export status and controls. Release-keyed operation
/// details live in `StorageTransferInspector`.
struct OutputSection: View {
    @Environment(OutputStore.self)
    private var outputStore
    @Environment(Outputs.self)
    private var outputs

    var body: some View {
        let snapshot = outputStore.snapshot
        if !snapshot.outputs.isEmpty {
            Divider()
            QueueSectionHeader(
                icon: "square.and.arrow.up",
                title: "Export & Save",
                pauseRequested: snapshot.paused,
                pauseStatusText: snapshot.paused
                    ? String(localized: "Paused") : nil,
                summaryText: snapshot.summaryText,
                retryDisabled: snapshot.total.failed == 0,
                onSetPaused: { outputs.setOutputsPaused($0) },
                onRetry: { outputs.retryOutputs() }
            )
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
