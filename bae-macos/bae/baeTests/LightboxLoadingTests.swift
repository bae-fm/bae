import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The lightbox's image can come from storage or a catalog, so while it loads
/// the lightbox says so.
@MainActor
@Suite("Lightbox loading")
struct LightboxLoadingTests {
    @Test("an image still loading is named")
    func loadingIsNamed() async throws {
        let image = BridgeRemoteImageSet(
            url: "https://images.example/front-original.png",
            downscaled: []
        )
        // Bytes that never arrive keep the lightbox on its load.
        let images = ImageStore(fetchRemoteImage: { _, _ in
            try await Task.sleep(for: .seconds(3_600))
            throw CancellationError()
        })
        let item = LightboxItem(
            id: "front",
            label: "Front",
            image: .remote(image)
        )
        let cursor = try #require(Cursor(items: [item], preferring: "front"))
        let size = NSSize(width: 640, height: 480)
        try await SnapshotTestSupport.withHostedWindow(
            LightboxView(cursor: cursor, onUpdate: { _ in }, onDismiss: {})
                .environment(images)
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            let lines = try await SnapshotTestSupport.recognizedText(in: png)
                .map(\.text)
            #expect(lines.carrying("Loading the image"), "drew \(lines)")
        }
    }
}
