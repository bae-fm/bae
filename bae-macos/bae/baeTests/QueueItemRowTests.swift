import AppKit
import BaeKit
import SwiftUI
import Testing
import Vision

@testable import bae

@Suite("Queue item credits")
struct QueueItemRowTests {
    @MainActor
    @Test(
        "a compilation track renders its title, artist, and album on separate lines"
    )
    func rendersTrackCredits() async throws {
        let item = QueueItem(
            bridge: BridgeQueueEntry(
                entryId: "entry",
                trackId: "track",
                title: "Track Title",
                artistNames: "Track Artist",
                durationClock: nil,
                albumTitle: "Compilation Album",
                coverImage: nil
            )
        )
        let size = NSSize(width: 420, height: 90)
        let hosted = SnapshotTestSupport.hostInWindow(
            QueueItemRow(
                item: item,
                isHovered: false,
                onHoverChanged: { _ in },
                onSkipTo: { _ in },
                onRemove: { _ in }
            )
            .environment(ImageStore.stub())
            .preferredColorScheme(.light)
            .padding(8)
            .background(.white),
            size: size
        )
        hosted.window.isReleasedWhenClosed = false
        defer { hosted.window.close() }
        await SnapshotTestSupport.settle(hosted.host)
        let png = try await SnapshotTestSupport.capturePNG(
            hosted.host,
            size: size
        )
        let lines =
            try await SnapshotTestSupport.recognizedText(
                in: png,
                languages: ["en-US"]
            )
            .map(\.text)
        #expect(lines.contains("Track Title"))
        #expect(lines.contains("Track Artist"))
        #expect(lines.contains("Compilation Album"))
    }
}
