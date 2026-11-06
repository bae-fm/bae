import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A step line keeps the room the step it shows takes when SwiftUI updates it
/// with the stream it already follows, and shows its idle content exactly
/// while its stream holds no step.
@MainActor
@Suite("Step progress line")
struct StepProgressLineTests {
    @Test("a running step keeps its line's height")
    func runningStepKeepsItsHeight() async throws {
        let size = NSSize(width: 360, height: 120)
        try await SnapshotTestSupport.withHostedWindow(
            StepProgressLine(
                steps: LatestStream.closingSteps(holding: .stoppingSync).only
            ) {
                EmptyView()
            }
            .frame(width: size.width, height: size.height, alignment: .top),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let line = try #require(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? StepProgressNSView<BridgeClosingStep> }
                    .first
            )
            #expect(line.steppingHeight > 0)
            #expect(
                abs(line.frame.height - line.steppingHeight) < 1,
                "sized \(line.frame.height) for \(line.steppingHeight)"
            )
            let words = SnapshotTestSupport.descendants(of: line)
                .compactMap { ($0 as? NSTextField)?.stringValue }
            #expect(
                words.contains { $0.hasPrefix("Stopping sync") },
                "drew \(words)"
            )
        }
    }

    @Test("a step running as the line appears shows from its first layout")
    func stepRunningAtFirstLayout() async throws {
        let size = NSSize(width: 360, height: 120)
        try await SnapshotTestSupport.withHostedWindow(
            StepProgressLine(
                steps: LatestStream.closingSteps(holding: .stoppingSync).only
            ) {
                IdleMarker()
            }
            .frame(width: size.width, height: size.height, alignment: .top),
            size: size
        ) { _, host in
            // No turn of the run loop: what the line knows as it appears is
            // all the first layout has.
            host.layoutSubtreeIfNeeded()
            #expect(!Self.showsIdle(host))
        }
    }

    @Test("a step starting or ending in the stream swaps the idle content")
    func stepStartingAndEnding() async throws {
        let size = NSSize(width: 360, height: 120)
        let steps = LatestStream<BridgeClosingStep>.written()
        try await SnapshotTestSupport.withHostedWindow(
            StepProgressLine(steps: steps.stream.only) {
                IdleMarker()
            }
            .frame(width: size.width, height: size.height, alignment: .top),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            #expect(Self.showsIdle(host))

            steps.set(.stoppingSync)
            try await Wait.untilLaidOut(host) { !Self.showsIdle(host) }

            steps.remove()
            try await Wait.untilLaidOut(host) { Self.showsIdle(host) }
        }
    }

    private static func showsIdle(_ host: NSView) -> Bool {
        host.layoutSubtreeIfNeeded()
        return SnapshotTestSupport.descendants(of: host)
            .contains { $0 is IdleMarkerNSView }
    }
}

/// Idle content a test finds in the hosted tree.
private final class IdleMarkerNSView: NSView {}

private struct IdleMarker: NSViewRepresentable {
    func makeNSView(context _: Context) -> IdleMarkerNSView {
        IdleMarkerNSView()
    }

    func updateNSView(_: IdleMarkerNSView, context _: Context) {}
}
