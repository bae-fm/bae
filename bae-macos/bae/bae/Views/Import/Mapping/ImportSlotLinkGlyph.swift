import BaeKit
import SwiftUI

/// The slot row's link column: one character carrying the pairing state.
///
/// Core hands over a typed span, never a glyph — which character draws a run is
/// this platform's choice. A container carved into several slots reads as one
/// unbroken run down the column, which is the whole point: the rows a single
/// file backs must be legible as a group without counting them.
struct ImportSlotLinkGlyph: View {
    let file: BridgeSlotFile?
    /// Whether the source names a position for this row.
    let hasPosition: Bool

    var body: some View {
        Text(verbatim: glyph)
            .font(.system(size: 12, design: .monospaced))
            .foregroundStyle(
                paired
                    ? AnyShapeStyle(Theme.accent) : AnyShapeStyle(.quaternary)
            )
            .accessibilityHidden(true)
    }

    /// A row is paired when both sides account for it: audio on disk and a
    /// track the source names.
    private var paired: Bool { file != nil && hasPosition }

    private var glyph: String {
        guard let file else { return "\u{254C}" }
        switch file.span {
        case .whole: return paired ? "\u{2501}" : "\u{254C}"
        case .containerStart: return "\u{2533}"
        case .containerMiddle: return "\u{2503}"
        case .containerEnd: return "\u{253B}"
        }
    }
}

/// The slot row's two lengths: the file's own over the source's.
///
/// A missing number is an em dash, never a zero — "unknown" and "no audio" are
/// different facts. When core says the two disagree the pair is marked, because
/// that is the one thing a complete-but-wrong pairing shows. It marks; it never
/// disables.
///
/// How far apart is far enough is core's call: it is a judgement about how much
/// two rips of one track may legitimately differ, and the other desktop surface
/// has to reach the same answer, so the rule lives in one place and both ask
/// it. Asked per render rather than carried on the slot, because re-pointing a
/// row at a different file gives it two new lengths.
struct ImportSlotLengths: View {
    let probedMs: UInt64?
    let sourceMs: UInt64?

    private var diverges: Bool {
        bridgeLengthsDisagree(probedMs: probedMs, sourceMs: sourceMs)
    }

    var body: some View {
        VStack(alignment: .trailing, spacing: 0) {
            Text(importDurationText(probedMs))
                .font(.system(size: 12))
                .monospacedDigit()
                .foregroundStyle(
                    diverges ? AnyShapeStyle(.orange) : AnyShapeStyle(.primary)
                )
            Text(importDurationText(sourceMs))
                .font(.system(size: 10.5))
                .monospacedDigit()
                .foregroundStyle(.tertiary)
        }
        .help(diverges ? String(localized: "Lengths differ") : "")
    }
}
