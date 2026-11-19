import BaeKit
import SwiftUI

/// How much this Mac keeps: the total, one bar split by kind, and a line for
/// each kind with its size. Draws what it is given and reads nothing else.
struct MacStorageUsage: View {
    let usage: StorageUsage

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
            let gaps = Self.gap * CGFloat(max(usage.segments.count - 1, 0))
            let room = max(proxy.size.width - gaps, 0)
            HStack(spacing: Self.gap) {
                ForEach(Array(usage.segments.enumerated()), id: \.element.id) {
                    index,
                    segment in
                    let last = usage.segments.count - 1
                    UnevenRoundedRectangle(
                        topLeadingRadius: index == 0 ? 4 : 2,
                        bottomLeadingRadius: index == 0 ? 4 : 2,
                        bottomTrailingRadius: index == last ? 4 : 2,
                        topTrailingRadius: index == last ? 4 : 2
                    )
                    .fill(segment.kind.color)
                    .frame(width: max(Self.thinnest, room * segment.share))
                }
            }
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
