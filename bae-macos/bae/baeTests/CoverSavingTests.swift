import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Saving a cover can download it or read it from the release's storage, so
/// the picker says so while it saves.
@MainActor
@Suite("Cover saving")
struct CoverSavingTests {
    @Test("a running save is named")
    func savingIsNamed() async throws {
        let size = NSSize(width: 1_100, height: 760)
        try await SnapshotTestSupport.withHostedWindow(
            CoverGalleryView(
                remoteItems: .linked([]),
                releaseItems: [],
                selectedCover: nil,
                isSaving: true,
                onSelect: { _ in },
                onDone: {}
            )
            .environment(PreviewData.artImageStore())
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            let lines = try await SnapshotTestSupport.recognizedText(in: png)
                .map(\.text)
            #expect(lines.carrying("Saving the cover"), "drew \(lines)")
        }
    }
}
