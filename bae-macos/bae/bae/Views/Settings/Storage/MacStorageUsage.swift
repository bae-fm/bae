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
                    .themeText(.figure)
                Text("on this Mac")
                    .themeText(.detail)
                    .foregroundStyle(Theme.textSecondary)
            }
            bar
            legend
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var bar: some View {
        ShareStack(spacing: Self.gap, thinnest: Self.thinnest) {
            ForEach(usage.segments) { segment in
                RoundedRectangle(cornerRadius: ThemeRadius.bar)
                    .fill(segment.kind.color)
                    .layoutValue(key: ShareKey.self, value: segment.share)
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

private struct ShareKey: LayoutValueKey {
    static let defaultValue: Double = 0
}

/// A row that gives each child `ShareKey`'s part of the width left after the
/// spacing, at least `thinnest` wide.
private struct ShareStack: Layout {
    let spacing: CGFloat
    let thinnest: CGFloat

    func sizeThatFits(
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout ()
    ) -> CGSize {
        CGSize(width: proposal.width ?? 0, height: proposal.height ?? 0)
    }

    func placeSubviews(
        in bounds: CGRect,
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout ()
    ) {
        let room = max(
            bounds.width - spacing * CGFloat(max(subviews.count - 1, 0)),
            0
        )
        let sized = widths(
            shares: subviews.map { $0[ShareKey.self] },
            room: room
        )
        var x = bounds.minX
        for (subview, width) in zip(subviews, sized) {
            subview.place(
                at: CGPoint(x: x, y: bounds.minY),
                proposal: ProposedViewSize(width: width, height: bounds.height)
            )
            x += width + spacing
        }
    }

    /// Each share's width in `room`: its share of it, except a segment that
    /// share would leave thinner than `thinnest`, which keeps `thinnest`
    /// and leaves the others the room that is left, so the segments fill
    /// the bar and no more.
    private func widths(shares: [Double], room: CGFloat) -> [CGFloat] {
        var pinned = Set<Int>()
        while true {
            let free = max(room - thinnest * CGFloat(pinned.count), 0)
            let total = shares.indices.filter { !pinned.contains($0) }
                .reduce(0) { $0 + shares[$1] }
            let thin = shares.indices.filter {
                !pinned.contains($0) && total > 0
                    && free * shares[$0] / total < thinnest
            }
            if thin.isEmpty {
                return shares.indices.map {
                    pinned.contains($0)
                        ? thinnest : (total > 0 ? free * shares[$0] / total : 0)
                }
            }
            pinned.formUnion(thin)
        }
    }
}
