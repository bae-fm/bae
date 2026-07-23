import BaeKit
import SwiftUI

/// Master progress strip: filled progress bar + bytes done/total, throughput,
/// and ETA. All three labels are pre-formatted by core.
struct OutboxTotalProgress: View {
    let snapshot: BridgeOutboxSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ProgressTrackBar(progress: snapshot.total.fraction)
                // Dim the bar while paused so the visual matches the "Paused"
                // chip in the band above — paused-but-mid-progress reads as
                // active otherwise.
                .opacity(snapshot.paused ? 0.4 : 1)
            HStack(spacing: 8) {
                Text(snapshot.total.bytesText)
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
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
            snapshot: PreviewData.outboxSnapshot(paused: true)
        )
        .frame(width: 700)
    }
#endif
