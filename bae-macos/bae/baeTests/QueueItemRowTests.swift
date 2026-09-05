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
        let bitmap = try #require(NSBitmapImageRep(data: png))
        let cgImage = try #require(bitmap.cgImage)
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.recognitionLanguages = ["en-US"]
        try VNImageRequestHandler(cgImage: cgImage).perform([request])
        let observations = try #require(request.results)
        let lines = observations.compactMap {
            $0.topCandidates(1).first?.string
        }
        #expect(lines.contains("Track Title"))
        #expect(lines.contains("Track Artist"))
        #expect(lines.contains("Compilation Album"))
    }
}
