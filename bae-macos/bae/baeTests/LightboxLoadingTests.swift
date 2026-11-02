import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The lightbox's image can come from storage or a catalog, so while it loads
/// the lightbox spins, saying so; the words are the
/// `artwork-lightbox-loading` shot's.
@MainActor
@Suite("Lightbox loading")
struct LightboxLoadingTests {
    @Test("an image still loading spins")
    func loadingSpins() async throws {
        let size = NSSize(width: 640, height: 480)
        try await SnapshotTestSupport.withHostedWindow(
            LightboxView<LightboxItem>.loadingPreview()
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let spinners = SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSProgressIndicator }
            #expect(spinners.count == 1)
        }
    }
}
