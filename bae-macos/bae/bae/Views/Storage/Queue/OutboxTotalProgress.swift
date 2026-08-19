import BaeKit
import SwiftUI

/// Master progress strip. The bar fills with the bytes of the phase the queue
/// is in — source bytes while anything is still being prepared, provider bytes
/// once every exact provider size is known — and its caption names that phase
/// and counts the same bytes.
struct OutboxTotalProgress: View {
    let snapshot: BridgeOutboxSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ProgressTrackBar(progress: snapshot.total.bar?.fraction ?? 0)
                // Dim the bar while paused so the visual matches the "Paused"
                // chip in the band above — paused-but-mid-progress reads as
                // active otherwise.
                .opacity(snapshot.pauseState == .paused ? 0.4 : 1)
            HStack(spacing: 8) {
                if let bar = snapshot.total.bar {
                    Text(bar.text)
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
