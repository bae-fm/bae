import SwiftUI

/// The header's "Identifying N / total" line and thin bar, fed by the queue
/// sweep's progress event. The caller hides this entirely once there is
/// nothing left to say — see `ImportCandidateListContent` — rather than this
/// view deciding that for itself.
struct QueueProgressView: View {
    let identified: UInt32
    let total: UInt32

    private var fraction: Double {
        total == 0 ? 1 : Double(identified) / Double(total)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack {
                Text("Identifying")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                Spacer()
                Text(
                    verbatim:
                        "\(identified.formatted()) / \(total.formatted())"
                )
                .font(.system(size: 12))
                .monospacedDigit()
                .foregroundStyle(.secondary)
            }
            ProgressTrackBar(progress: fraction, trackHeight: 3)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Queue progress") {
        QueueProgressView(identified: 112, total: 130)
            .padding()
            .frame(width: 280)
            .windowBackground()
    }
#endif
