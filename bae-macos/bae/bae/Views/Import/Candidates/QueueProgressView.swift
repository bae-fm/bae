import SwiftUI

/// The header's "Identifying N / total" line and thin bar, fed by the queue
/// sweep's progress event. The caller hides this entirely once there is
/// nothing left to say — see `ImportCandidateListContent` — rather than this
/// view deciding that for itself.
///
/// The line is a control, not a label. The candidates the count is waiting on
/// are rows somewhere in the queue, and a number that sits still while giving
/// no way to reach what it is waiting on is the frustrating half of this pane.
/// Tapping it goes to the first one.
struct QueueProgressView: View {
    let identified: UInt32
    let total: UInt32
    /// Go to the first candidate with no verdict yet. Nil when there is none
    /// to go to, which is also when the count has nothing left to wait on.
    let onGoToUnidentified: (() -> Void)?

    private var fraction: Double {
        total == 0 ? 1 : Double(identified) / Double(total)
    }

    var body: some View {
        Button {
            onGoToUnidentified?()
        } label: {
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
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(onGoToUnidentified == nil)
        .help("Go to a candidate still being identified")
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Queue progress") {
        QueueProgressView(
            identified: 112,
            total: 130,
            onGoToUnidentified: {}
        )
        .padding()
        .frame(width: 280)
        .windowBackground()
    }
#endif
