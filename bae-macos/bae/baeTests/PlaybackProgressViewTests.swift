import AppKit
import SwiftUI
import Testing

@testable import BaeKit
@testable import bae

/// A track position as its stream holds it.
private func position(
    _ positionMs: Int64,
    of durationMs: UInt64?,
    progress: Double
) -> BridgeTrackPosition {
    BridgeTrackPosition(
        positionMs: positionMs,
        rate: 0,
        anchoredAtNs: 0,
        durationMs: durationMs
    )
}

@Suite("Playback timeline", .serialized)
@MainActor
struct PlaybackProgressViewTests {
    @Test("a position that starts after none displays both clocks")
    func positionAfterNone() async throws {
        let positions = LatestStream<BridgeTrackPosition>.written()
        try await SnapshotTestSupport.withHostedWindow(
            PlaybackProgressView(
                position: positions.stream.source(for: "track"),
                buffered: LatestStream.trackBuffered().source(for: "track"),
                showRemainingTime: false,
                onSeek: { _ in },
                onToggleRemainingTime: {}
            ),
            size: NSSize(width: 460, height: 40)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let descendants = SnapshotTestSupport.descendants(of: host)
            let slider = try #require(
                descendants.compactMap { $0 as? SeekSlider }.first
            )
            let labels = descendants.compactMap { $0 as? NSTextField }
            #expect(labels.map(\.stringValue) == ["0:00", "0:00"])

            positions.set(
                position(45_000, of: 180_000, progress: 0.25),
                for: "track"
            )
            try await Wait.untilText(of: labels, is: ["0:45", "3:00"])
            #expect(slider.doubleValue == 0.25)
        }
    }

    @Test(
        "a newly mounted bar shows the position its stream holds",
        arguments: [false, true]
    )
    func mountedBarAndClockModes(showRemaining: Bool) async throws {
        let positions = LatestStream<BridgeTrackPosition>.written()
        positions.set(
            position(25_000, of: 100_000, progress: 0.25),
            for: "track"
        )
        try await SnapshotTestSupport.withHostedWindow(
            PlaybackProgressView(
                position: positions.stream.source(for: "track"),
                buffered: LatestStream.trackBuffered().source(for: "track"),
                showRemainingTime: showRemaining,
                onSeek: { _ in },
                onToggleRemainingTime: {}
            ),
            size: NSSize(width: 460, height: 40)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let descendants = SnapshotTestSupport.descendants(of: host)
            let bar = try #require(
                descendants.compactMap { $0 as? SeekBarNSView }.first
            )
            let labels = descendants.compactMap { $0 as? NSTextField }
            #expect(
                labels.map(\.stringValue) == [
                    showRemaining ? "-1:15" : "0:25", "1:40",
                ]
            )

            positions.set(
                position(-1_250, of: 100_000, progress: 0),
                for: "track"
            )
            bar.tracks.frame()
            #expect(labels.map(\.stringValue) == ["-0:02", "1:40"])

            positions.set(position(25_000, of: nil, progress: 0), for: "track")
            bar.tracks.frame()
            #expect(labels.map(\.stringValue) == ["0:25", ""])
        }
    }

    @Test("preference refreshes keep the timeline and cannot undo an end")
    func preferenceRefresh() async throws {
        let positions = LatestStream<BridgeTrackPosition>.written()
        positions.set(
            position(45_000, of: 180_000, progress: 0.25),
            for: "track"
        )
        func content(showRemaining: Bool) -> some View {
            PlaybackProgressView(
                position: positions.stream.source(for: "track"),
                buffered: LatestStream.trackBuffered().source(for: "track"),
                showRemainingTime: showRemaining,
                onSeek: { _ in },
                onToggleRemainingTime: {}
            )
        }
        try await SnapshotTestSupport.withHostedWindow(
            content(showRemaining: false),
            size: NSSize(width: 460, height: 40)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let descendants = SnapshotTestSupport.descendants(of: host)
            let bar = try #require(
                descendants.compactMap { $0 as? SeekBarNSView }.first
            )
            let labels = descendants.compactMap { $0 as? NSTextField }
            #expect(labels.map(\.stringValue) == ["0:45", "3:00"])

            host.rootView = content(showRemaining: true)
            try await SnapshotTestSupport.settle(host)
            #expect(labels.map(\.stringValue) == ["-2:15", "3:00"])
            #expect(
                SnapshotTestSupport.descendants(of: host)
                    .contains { $0 === bar }
            )

            positions.remove("track")
            try await Wait.untilText(of: labels, is: ["0:00", "0:00"])
            host.rootView = content(showRemaining: false)
            try await SnapshotTestSupport.settle(host)
            #expect(labels.map(\.stringValue) == ["0:00", "0:00"])

            positions.set(
                position(120_000, of: 240_000, progress: 0.5),
                for: "track"
            )
            try await Wait.untilText(of: labels, is: ["2:00", "4:00"])
        }
    }

    /// A seek the person drops on the bar changes nothing the bar draws: core
    /// names the seek's target as the track's position when it accepts the
    /// seek, and the bar draws each position the stream holds.
    @Test("a seek leaves the bar drawing the stream's position")
    func seekLeavesTheBarOnTheStream() async throws {
        let positions = LatestStream<BridgeTrackPosition>.written()
        positions.set(
            position(45_000, of: 180_000, progress: 0.25),
            for: "track"
        )
        var seeks: [Double] = []
        try await SnapshotTestSupport.withHostedWindow(
            PlaybackProgressView(
                position: positions.stream.source(for: "track"),
                buffered: LatestStream.trackBuffered().source(for: "track"),
                showRemainingTime: false,
                onSeek: { seeks.append($0) },
                onToggleRemainingTime: {}
            ),
            size: NSSize(width: 460, height: 40)
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let descendants = SnapshotTestSupport.descendants(of: host)
            let slider = try #require(
                descendants.compactMap { $0 as? SeekSlider }.first
            )
            let bar = try #require(
                descendants.compactMap { $0 as? SeekBarNSView }.first
            )
            let labels = descendants.compactMap { $0 as? NSTextField }
            try clickMiddle(of: slider, in: window)
            #expect(seeks.count == 1)
            #expect(abs(try #require(seeks.first) - 0.5) < 0.01)
            #expect(labels.map(\.stringValue) == ["0:45", "3:00"])

            positions.set(
                position(92_000, of: 180_000, progress: 0.51),
                for: "track"
            )
            bar.tracks.frame()
            #expect(labels.map(\.stringValue) == ["1:32", "3:00"])
        }
    }

    @Test("import audition displays both clocks, and none once it ends")
    func previewClocks() async throws {
        let target = BridgePreviewTarget(
            path: Data("/Music/Track.flac".utf8),
            name: "Track.flac",
            startSample: 0,
            endSample: nil
        )
        let preview = LatestStream<BridgePreviewPosition>.written()
        try await SnapshotTestSupport.withHostedWindow(
            PreviewProgressView(onSeek: { _ in })
                .environment(\.previewPosition, preview.stream.only),
            size: NSSize(width: 460, height: 40)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let descendants = SnapshotTestSupport.descendants(of: host)
            let slider = try #require(
                descendants.compactMap { $0 as? SeekSlider }.first
            )
            let bar = try #require(
                descendants.compactMap { $0 as? SeekBarNSView }.first
            )
            let labels = descendants.compactMap { $0 as? NSTextField }

            preview.set(
                BridgePreviewPosition(
                    target: target,
                    durationMs: 180_000,
                    positionMs: 45_000,
                    progress: 0.25
                )
            )
            try await Wait.untilText(of: labels, is: ["0:45", "3:00"])
            #expect(slider.doubleValue == 0.25)

            preview.set(
                BridgePreviewPosition(
                    target: target,
                    durationMs: 240_000,
                    positionMs: 120_000,
                    progress: 0.5
                )
            )
            bar.previews.frame()
            #expect(labels.map(\.stringValue) == ["2:00", "4:00"])
            #expect(slider.doubleValue == 0.5)

            preview.remove()
            try await Wait.untilText(of: labels, is: ["", ""])
            #expect(slider.doubleValue == 0)
        }
    }
}

@Suite("Seek bar drag", .serialized)
@MainActor
struct SeekBarDragTests {
    /// While the person drags, the time follows the dragged position and the
    /// positions core keeps sending do not move it, whether the bar counts up
    /// or down; at the drop it stays until core applies the seek.
    @Test(
        "the time follows the drag, not the playing position",
        arguments: [false, true]
    )
    func timeFollowsDrag(showRemaining: Bool) async throws {
        let positions = LatestStream<BridgeTrackPosition>.written()
        positions.set(
            position(45_000, of: 180_000, progress: 0.25),
            for: "track"
        )
        try await SnapshotTestSupport.withHostedWindow(
            PlaybackProgressView(
                position: positions.stream.source(for: "track"),
                buffered: LatestStream.trackBuffered().source(for: "track"),
                showRemainingTime: showRemaining,
                onSeek: { _ in },
                onToggleRemainingTime: {}
            ),
            size: NSSize(width: 460, height: 40)
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let descendants = SnapshotTestSupport.descendants(of: host)
            let slider = try #require(
                descendants.compactMap { $0 as? SeekSlider }.first
            )
            let bar = try #require(
                descendants.compactMap { $0 as? SeekBarNSView }.first
            )
            let labels = descendants.compactMap { $0 as? NSTextField }
            let probe = DragProbe(slider: slider) {
                positions.set(
                    position(46_000, of: 180_000, progress: 0.26),
                    for: "track"
                )
                bar.tracks.frame()
            }
            try clickMiddle(of: slider, in: window)
            #expect(probe.shownDuringDrag == [showRemaining ? "-1:30" : "1:30"])
            #expect(
                labels.map(\.stringValue)
                    == [showRemaining ? "-1:30" : "1:30", "3:00"]
            )
        }
    }
}

@Suite("Seek bar held parts")
@MainActor
struct SeekBarHeldPartsTests {
    /// The bar draws the parts of its track whose bytes are here as its
    /// stream holds them, and each change after.
    @Test("the bar draws the held parts of its track")
    func heldParts() async throws {
        let positions = LatestStream<BridgeTrackPosition>.written()
        positions.set(
            position(45_000, of: 180_000, progress: 0.25),
            for: "track"
        )
        let buffered = LatestStream<BridgeTrackBuffered>.written()
        let start = [BridgeTrackSpan(start: 0, end: 0.4)]
        buffered.set(BridgeTrackBuffered(ranges: start), for: "track")
        try await SnapshotTestSupport.withHostedWindow(
            PlaybackProgressView(
                position: positions.stream.source(for: "track"),
                buffered: buffered.stream.source(for: "track"),
                showRemainingTime: false,
                onSeek: { _ in },
                onToggleRemainingTime: {}
            ),
            size: NSSize(width: 460, height: 40)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let descendants = SnapshotTestSupport.descendants(of: host)
            let slider = try #require(
                descendants.compactMap { $0 as? SeekSlider }.first
            )
            let bar = try #require(
                descendants.compactMap { $0 as? SeekBarNSView }.first
            )
            #expect(slider.held == start)

            let gapped = [
                BridgeTrackSpan(start: 0, end: 0.4),
                BridgeTrackSpan(start: 0.6, end: 0.8),
            ]
            buffered.set(BridgeTrackBuffered(ranges: gapped), for: "track")
            bar.buffered.frame()
            #expect(slider.held == gapped)

            buffered.remove("track")
            bar.buffered.frame()
            #expect(slider.held.isEmpty)
        }
    }
}

@Suite("Queue strip")
@MainActor
struct ProgressStripTests {
    /// A moving anchor draws the position it puts the track at now, with no
    /// further value written to the stream.
    @Test("the queue strip draws a moving anchor from the clock")
    func stripDrawsAMovingAnchor() throws {
        let positions = LatestStream<BridgeTrackPosition>.written()
        positions.set(
            BridgeTrackPosition(
                positionMs: 45_000,
                rate: 1,
                anchoredAtNs: FrameClock.nowNs() - 30_000_000_000,
                durationMs: 180_000
            ),
            for: "track"
        )
        let strip = ProgressStripNSView(accent: .controlAccentColor)
        strip.follow(positions.stream.source(for: "track"))
        let label = try #require(
            SnapshotTestSupport.descendants(of: strip)
                .compactMap { $0 as? NSTextField }.first
        )
        #expect(["1:15", "1:16"].contains(label.stringValue))
        strip.stopFollowing()
    }

    /// The strip draws each position its track's stream holds, the target of
    /// a seek among them.
    @Test("the queue strip draws its track's stream")
    func stripDrawsTheStream() throws {
        let positions = LatestStream<BridgeTrackPosition>.written()
        positions.set(
            position(45_000, of: 180_000, progress: 0.25),
            for: "track"
        )
        let strip = ProgressStripNSView(accent: .controlAccentColor)
        strip.follow(positions.stream.source(for: "track"))
        let label = try #require(
            SnapshotTestSupport.descendants(of: strip)
                .compactMap { $0 as? NSTextField }.first
        )
        #expect(label.stringValue == "0:45")

        positions.set(
            position(92_000, of: 180_000, progress: 0.51),
            for: "track"
        )
        strip.following.frame()
        #expect(label.stringValue == "1:32")
        strip.stopFollowing()
    }
}

@MainActor
private func clickMiddle(of slider: SeekSlider, in window: NSWindow) throws {
    let cell = try #require(slider.cell as? NSSliderCell)
    let rect = cell.barRect(flipped: slider.isFlipped)
    let point = slider.convert(NSPoint(x: rect.midX, y: rect.midY), to: nil)
    try HostedInput.track(slider, at: point)
}

/// Stands in for the slider's target during a press: forwards each action
/// to the bar, then makes `during` happen mid-drag and records the leading
/// label after it.
@MainActor
private final class DragProbe: NSObject {
    private let slider: SeekSlider
    private let target: AnyObject?
    private let action: Selector?
    private let during: () -> Void
    private(set) var shownDuringDrag: [String] = []

    init(slider: SeekSlider, during: @escaping () -> Void) {
        self.slider = slider
        self.target = slider.target
        self.action = slider.action
        self.during = during
        super.init()
        slider.target = self
        slider.action = #selector(fired(_:))
    }

    @objc
    private func fired(_ sender: SeekSlider) {
        if let action {
            NSApp.sendAction(action, to: target, from: sender)
        }
        during()
        let host = slider.superview?.subviews.compactMap { $0 as? NSTextField }
        shownDuringDrag.append(host?.first?.stringValue ?? "")
    }
}
