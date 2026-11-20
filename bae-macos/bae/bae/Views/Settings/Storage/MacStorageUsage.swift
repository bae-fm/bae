import BaeKit
import SwiftUI

/// How much this Mac keeps: the total, one bar split by kind, and a line for
/// each kind with its size. Draws what it is given and reads nothing else.
struct MacStorageUsage: View {
    let usage: StorageUsage

    /// The space between two neighbouring segments.
    private static let gap: CGFloat = 2
    private static let thinnest: CGFloat = 4

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            HStack(alignment: .firstTextBaseline, spacing: ThemeSpace.inline) {
                Text(verbatim: usage.total.fileSizeText)
                    .themeText(.title)
                Text("on this Mac")
                    .foregroundStyle(.secondary)
            }
            bar
            legend
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var bar: some View {
        GeometryReader { proxy in
            HStack(spacing: 0) {
                ForEach(usage.segments) { segment in
                    Rectangle()
                        .fill(segment.kind.color)
                        .frame(
                            width: max(
                                Self.thinnest,
                                proxy.size.width * segment.share
                            )
                        )
                        // Each segment's edges give up half a gap, so two
                        // neighbours leave one gap between them.
                        .mask {
                            Rectangle().padding(.horizontal, Self.gap / 2)
                        }
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: 4))
        }
        .frame(height: 8)
        // VoiceOver reads the bar as one image, labelled with each kind and
        // its size.
        .accessibilityElement()
        .accessibilityAddTraits(.isImage)
        .accessibilityLabel(
            Text(
                verbatim: usage.segments
                    .map { "\($0.kind.title) \($0.bytes.fileSizeText)" }
                    .joined(separator: ", ")
            )
        )
    }

    private var legend: some View {
        FlowLayout(spacing: ThemeSpace.group, rowSpacing: ThemeSpace.inline) {
            ForEach(usage.segments) { segment in
                MacUsageKey(
                    color: segment.kind.color,
                    title: segment.kind.title,
                    size: segment.bytes.fileSizeText
                )
            }
        }
    }
}
