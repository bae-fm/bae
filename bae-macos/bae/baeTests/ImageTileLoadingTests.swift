import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A tile whose image is still loading is a plain tile, without a spinner.
@MainActor
@Suite("Image tile loading")
struct ImageTileLoadingTests {
    @Test("a loading image draws no spinner")
    func loadingDrawsNoSpinner() async throws {
        // Bytes that never arrive keep the tile on its load.
        let images = ImageStore(fetchRemoteImage: { _, _ in
            try await Task.neverAnswers()
        })
        let side: CGFloat = 120
        let size = NSSize(width: side, height: side)
        try await SnapshotTestSupport.withHostedWindow(
            ImageView(
                content: .remote(
                    BridgeRemoteImageSet(
                        url: "https://images.example/front-original.png",
                        downscaled: []
                    )
                ),
                pointSize: side
            )
            .frame(width: side, height: side)
            .environment(images),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let spinners = SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSProgressIndicator }
            #expect(spinners.isEmpty, "drew \(spinners)")
        }
    }
}
