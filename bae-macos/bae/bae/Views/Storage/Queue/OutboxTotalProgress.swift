import BaeKit
import SwiftUI

/// Master progress strip. The bar covers the complete two-stage pipeline;
/// its caption shows exact bytes for the phase currently doing I/O.
struct OutboxTotalProgress: View {
    let snapshot: BridgeOutboxSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ProgressTrackBar(progress: snapshot.total.fraction)
                // Dim the bar while paused so the visual matches the "Paused"
                // chip in the band above — paused-but-mid-progress reads as
                // active otherwise.
                .opacity(snapshot.pauseState == .paused ? 0.4 : 1)
            HStack(spacing: 8) {
                if let stageBytesText = snapshot.total.stageBytesText {
                    Text(stageBytesText)
                        .font(.caption)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                if !snapshot.throughputText.isEmpty {
                    Text(verbatim: "·").foregroundStyle(.tertiary)
                    Text(snapshot.throughputText)
                        .font(.caption)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                if !snapshot.etaText.isEmpty {
                    Text(verbatim: "·").foregroundStyle(.tertiary)
                    Text(snapshot.etaText)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }
        }
        .padding(.horizontal)
        .padding(.vertical, 4)
    }
}

#if DEBUG
    #Preview("Active") {
        OutboxTotalProgress(snapshot: PreviewData.outboxSnapshot())
            .frame(width: 700)
    }

    #Preview("Paused") {
        OutboxTotalProgress(
            snapshot: PreviewData.outboxSnapshot(pauseState: .paused)
        )
        .frame(width: 700)
    }
#endif
