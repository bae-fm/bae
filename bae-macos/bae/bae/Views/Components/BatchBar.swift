import AppKit
import BaeKit
import OSLog
import SwiftUI

private let logger = Logger.bae("BatchBar")

/// A transfer line's bar: the share of its batch's bytes done, as the line's
/// row says, then for each file or release that bae is transferring a segment
/// that starts where the row says and spans its share, filled to the fraction
/// its stream holds. Each segment reads its key's latest value once per
/// display frame while the key's value changes, with no SwiftUI update; a key
/// the stream holds nothing for leaves its segment empty.
final class BatchBarNSView<Value: Sendable & Equatable>: NSView {
    /// One file or release that bae is transferring, and the fraction of it
    /// that its stream reports done. Each place in the row's list keeps its reader, so a file that
    /// follows another in its place reuses it.
    private struct Segment {
        var start: Double = 0
        var share: Double = 0
        var fraction: Double = 0
    }

    private let fraction: (Value) -> Double
    private var done: Double = 0
    private var segments: [Segment] = []
    private var followings: [FrameFollowing<Value>] = []

    var accent: NSColor {
        didSet { needsDisplay = true }
    }

    /// A bar in `accent` whose segments fill to `fraction` of each value
    /// their stream holds.
    init(accent: NSColor, fraction: @escaping (Value) -> Double) {
        self.accent = accent
        self.fraction = fraction
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    override func isAccessibilityElement() -> Bool { true }

    override func accessibilityRole() -> NSAccessibility.Role? {
        .progressIndicator
    }

    override func accessibilityValue() -> Any? {
        done.formatted(.percent.precision(.fractionLength(0)))
    }

    override var intrinsicContentSize: NSSize {
        NSSize(
            width: NSView.noIntrinsicMetric,
            height: ProgressTrackNSView.trackHeight
        )
    }

    /// Draw `bar`, each segment following its key's value in `stream`.
    func show(_ bar: BridgeBatchBar, stream: LatestStream<Value>) {
        done = bar.done
        while followings.count < bar.moving.count {
            let index = followings.count
            followings.append(
                FrameFollowing<Value>(view: self) { [weak self] value in
                    self?.show(value, at: index)
                }
            )
        }
        if segments.count > bar.moving.count {
            for following in followings[bar.moving.count...] {
                following.stop()
            }
            segments.removeLast(segments.count - bar.moving.count)
        }
        while segments.count < bar.moving.count {
            segments.append(Segment())
        }
        for (index, moving) in bar.moving.enumerated() {
            segments[index].start = moving.start
            segments[index].share = moving.share
            followings[index].follow(stream.source(for: moving.key))
        }
        needsDisplay = true
    }

    func stopFollowing() {
        for following in followings {
            following.stop()
        }
        segments = []
    }

    private func show(_ value: Value?, at index: Int) {
        guard segments.indices.contains(index) else {
            // `show(_:stream:)` stops each following past the row's length,
            // and a stopped following calls nothing.
            let detail =
                "a following called segment \(index) of \(segments.count)"
            logger.error("\(detail)")
            CoreBugReporter().report(operation: "BatchBar.show", detail: detail)
            return
        }
        segments[index].fraction = value.map(fraction) ?? 0
        needsDisplay = true
    }

    override func draw(_: NSRect) {
        let track = NSRect(
            x: 0,
            y: (bounds.height - ProgressTrackNSView.trackHeight) / 2,
            width: bounds.width,
            height: ProgressTrackNSView.trackHeight
        )
        ProgressTrackDrawing.drawTrack(in: track)
        ProgressTrackDrawing.drawFill(in: track, fraction: done, accent: accent)
        ProgressTrackDrawing.drawSpans(
            segments.map {
                BridgeTrackSpan(
                    start: $0.start,
                    end: $0.start + $0.share * $0.fraction
                )
            },
            in: track,
            color: accent
        )
    }
}

/// The SwiftUI view that wraps a transfer line's bar and passes it the line's
/// row and the stream its segments follow, never a value.
struct BatchBar<Value: Sendable & Equatable>: NSViewRepresentable {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.colorScheme)
    private var colorScheme
    let bar: BridgeBatchBar
    let stream: LatestStream<Value>
    /// How far a value of the stream has got, from 0 to 1, as core sends it.
    let fraction: (Value) -> Double

    func makeNSView(context _: Context) -> BatchBarNSView<Value> {
        let view = BatchBarNSView<Value>(accent: fill, fraction: fraction)
        view.show(bar, stream: stream)
        return view
    }

    func updateNSView(_ view: BatchBarNSView<Value>, context _: Context) {
        view.accent = fill
        view.show(bar, stream: stream)
    }

    static func dismantleNSView(
        _ view: BatchBarNSView<Value>,
        coordinator _: ()
    ) {
        view.stopFollowing()
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView _: BatchBarNSView<Value>,
        context _: Context
    ) -> CGSize? {
        CGSize(
            width: proposal.width ?? 0,
            height: ProgressTrackNSView.trackHeight
        )
    }

    private var fill: NSColor {
        NSColor(accent.color(in: colorScheme))
    }
}
