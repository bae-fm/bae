import SwiftUI

/// A line of progress: what is happening, then the bar filling the rest of
/// the line, then — when there is a count to show — how far along
/// ("112 / 130", "Uploading 3 MB of 221.2 MB"). Every surface that pairs a
/// bar with the text describing it says it this way, so an importing row, a
/// storage cell, and a release's storage band all read as one sentence.
///
/// The label takes the surrounding font: a sidebar row and a detail band set
/// their own size on the line and nothing else. A label that is not plain
/// text — the tinted upload-activity label — comes in through the view
/// initializer.
struct ProgressLine<Label: View>: View {
    /// 0...1 for a determinate fill; nil for the indeterminate marching pill.
    let progress: Double?
    /// The count after the bar. Nil when the line has only a phase to name.
    let detail: String?
    let label: Label

    init(
        progress: Double?,
        detail: String? = nil,
        @ViewBuilder label: () -> Label
    ) {
        self.progress = progress
        self.detail = detail
        self.label = label()
    }

    var body: some View {
        // A line too narrow for all three parts drops the count before it
        // clips the phase: a storage cell reads "1 uploading" and a bar, not
        // "1…ing" and a rate.
        ViewThatFits(in: .horizontal) {
            line(detail: detail)
            line(detail: nil)
        }
    }

    private func line(detail: String?) -> some View {
        HStack(spacing: 8) {
            label
                .lineLimit(1)
                .foregroundStyle(.secondary)
                // The label yields before the bar does, but never below the
                // bar's minimum: a long phase truncates in the middle rather
                // than squeezing the bar out of the line.
                .truncationMode(.middle)
                .layoutPriority(1)
            ProgressTrackBar(progress: progress)
                .frame(minWidth: 48)
            if let detail {
                Text(detail)
                    .monospacedDigit()
                    .lineLimit(1)
                    .foregroundStyle(.secondary)
                    .fixedSize()
            }
        }
    }
}

extension ProgressLine where Label == Text {
    init(_ label: String, progress: Double?, detail: String? = nil) {
        self.init(progress: progress, detail: detail) { Text(label) }
    }
}

#if DEBUG
    #Preview("Progress Line") {
        VStack(alignment: .leading, spacing: 14) {
            ProgressLine("Reading files", progress: 0.42, detail: "42%")
            ProgressLine("Importing…", progress: nil)
            ProgressLine(
                "Identifying",
                progress: 112 / 130,
                detail: "112 / 130"
            )
            ProgressLine(
                "Uploading 3 files",
                progress: 0.15,
                detail: "Uploading 3 MB of 221.2 MB"
            )
            ProgressLine(
                "A phase named at such length that it has to yield to the bar",
                progress: 0.6
            )
            .frame(width: 200)
        }
        .font(.system(size: 12.5))
        .padding()
        .frame(width: 360)
        .windowBackground()
    }
#endif
