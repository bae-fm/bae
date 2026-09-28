import AppKit
import SwiftUI
import Testing

@testable import bae

@Suite("Progress track", .serialized)
@MainActor
struct ProgressTrackTests {
    @Test("a bar made with no fraction marches from its first layout")
    func marchesFromCreation() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            ProgressTrackBar(progress: nil),
            size: NSSize(width: 200, height: 20)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            #expect(marching(try track(in: host)))
        }
    }

    @Test("a bar marches whenever its fraction is gone, and only then")
    func marchesWithoutAFraction() async throws {
        let bar = ProgressTrackNSView(
            progress: nil,
            accent: .controlAccentColor
        )
        bar.frame = NSRect(x: 0, y: 0, width: 200, height: 20)
        bar.needsLayout = true
        bar.layoutSubtreeIfNeeded()
        #expect(marching(bar))

        bar.progress = 0.4
        bar.layoutSubtreeIfNeeded()
        #expect(!marching(bar))

        bar.progress = nil
        bar.layoutSubtreeIfNeeded()
        #expect(marching(bar))
    }

    @Test("the queue's playback strip holds still before any position")
    func playbackStripStartsEmpty() throws {
        let strip = ProgressStripNSView(accent: .controlAccentColor)
        strip.frame = NSRect(x: 0, y: 0, width: 200, height: 20)
        strip.layoutSubtreeIfNeeded()
        #expect(!marching(try track(in: strip)))
    }

    private func track(in host: NSView) throws -> ProgressTrackNSView {
        try #require(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? ProgressTrackNSView }
                .first
        )
    }

    /// Whether a layer in the bar's tree is animating, which only the
    /// marching pill does.
    private func marching(_ bar: ProgressTrackNSView) -> Bool {
        func animating(_ layer: CALayer) -> Bool {
            layer.animationKeys()?.isEmpty == false
                || (layer.sublayers ?? []).contains(where: animating)
        }
        return bar.layer.map(animating) ?? false
    }
}
