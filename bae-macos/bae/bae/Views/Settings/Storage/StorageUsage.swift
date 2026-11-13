import BaeKit
import SwiftUI

/// What Settings → Storage's usage part draws, from what core measured: the
/// total and one segment for each kind that holds bytes, in the order core
/// lists them. A kind core leaves out is not drawn.
struct StorageUsage: Equatable {
    struct Segment: Equatable, Identifiable {
        let kind: BridgeStorageKind
        let bytes: UInt64
        /// This kind's part of the total, from 0 to 1.
        let share: Double

        var id: BridgeStorageKind { kind }
    }

    let total: UInt64
    let segments: [Segment]

    init(_ storage: BridgeDeviceStorage) {
        total = storage.total
        segments = storage.parts.map { part in
            Segment(
                kind: part.kind,
                bytes: part.bytes,
                share: storage.total == 0
                    ? 0 : Double(part.bytes) / Double(storage.total)
            )
        }
    }
}

extension BridgeStorageKind {
    /// The accent whose selection step colors this kind, the same in both
    /// themes: blue, gold, orchid and mint keep apart for colour-blind
    /// viewers and each reaches 3:1 on the surface.
    var accent: AccentChoice {
        switch self {
        case .pinned: .blue
        case .cache: .gold
        case .library: .orchid
        case .beforeReset: .mint
        }
    }

    var color: Color { accent.buttonColor }

    var title: String {
        switch self {
        case .pinned: String(localized: "Pinned")
        case .cache: String(localized: "Cache")
        case .library: String(localized: "Library")
        case .beforeReset: String(localized: "From before the reset")
        }
    }
}

/// How much this Mac keeps: the total, one bar split by kind, and a line for
/// each kind with its size. Draws what it is given and reads nothing else.
struct StorageUsageView: View {
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
        .accessibilityElement(children: .combine)
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
        .accessibilityHidden(true)
    }

    private var legend: some View {
        FlowLayout(spacing: ThemeSpace.group, rowSpacing: ThemeSpace.inline) {
            ForEach(usage.segments) { segment in
                HStack(spacing: ThemeSpace.inline) {
                    Circle()
                        .fill(segment.kind.color)
                        .frame(width: 8, height: 8)
                    Text(verbatim: segment.kind.title)
                        .foregroundStyle(.secondary)
                    Text(verbatim: segment.bytes.fileSizeText)
                        .monospacedDigit()
                }
                .themeText(.detail)
                .fixedSize()
            }
        }
    }
}
