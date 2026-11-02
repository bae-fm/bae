import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A step line keeps the room the step it shows takes when SwiftUI updates it
/// with the stream it already follows.
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
}
