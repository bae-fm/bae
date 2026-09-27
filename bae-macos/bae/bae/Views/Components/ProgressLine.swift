import BaeKit
import SwiftUI

/// A line of progress: the phase, a bar filling the rest of the line, and an
/// optional count ("112 / 130"), in the surrounding font.
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
        // A line too narrow for all three parts drops the count first.
        ViewThatFits(in: .horizontal) {
            line(detail: detail)
            line(detail: nil)
        }
    }

    private func line(detail: String?) -> some View {
        HStack(spacing: ThemeSpace.related) {
            label
                .lineLimit(1)
                .foregroundStyle(.secondary)
                // A long phase truncates rather than squeezing out the bar.
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
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
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
        .themeText(.body)
        .padding()
        .frame(width: 360)
        .windowBackground()
    }
#endif
