import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A step line keeps drawing the step it shows when SwiftUI updates it with
/// the stream it already follows.
@MainActor
@Suite("Step progress line")
struct StepProgressLineTests {
    @Test("a running step stays drawn")
    func runningStepIsDrawn() async throws {
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
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            let lines = try await SnapshotTestSupport.recognizedText(in: png)
                .map(\.text)
            #expect(lines.carrying("Stopping sync"), "drew \(lines)")
        }
    }
}
