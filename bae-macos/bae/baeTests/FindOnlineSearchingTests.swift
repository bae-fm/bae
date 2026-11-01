import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A catalog still answering a typed search can take a while, so its line
/// says it is searching it.
@MainActor
@Suite("Find online searching line")
struct FindOnlineSearchingTests {
    @Test("a catalog still searching is named as searching")
    func searchingCatalogIsNamed() async throws {
        let size = NSSize(width: 660, height: 460)
        try await SnapshotTestSupport.withHostedWindow(
            FindOnlineSearchResults(
                search: PreviewData.searchRunInFlight,
                isImporting: false,
                libraryStatuses: [:],
                selectedReleaseId: nil,
                loadingReleaseId: nil,
                onRetry: {},
                onSelect: { _ in }
            )
            .frame(width: size.width, height: size.height)
            .importPreviewEnvironment(),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            let lines = try await SnapshotTestSupport.recognizedText(in: png)
                .map(\.text)
            #expect(lines.carrying("Searching Discogs"), "drew \(lines)")
        }
    }
}
