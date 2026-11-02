import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A catalog still answering a typed search can take a while, so its line
/// spins, and says it is searching it; the words are the
/// `import-search-searching` shot's.
@MainActor
@Suite("Find online searching line")
struct FindOnlineSearchingTests {
    @Test("a catalog still searching spins on its line, and only then")
    func searchingCatalogSpins() async throws {
        #expect(try await spinners(PreviewData.searchRunInFlight) == 1)
        #expect(try await spinners(PreviewData.manualSearchRun) == 0)
    }

    private func spinners(_ search: BridgeCandidateSearch) async throws -> Int {
        let size = NSSize(width: 660, height: 460)
        return try await SnapshotTestSupport.withHostedWindow(
            FindOnlineSearchResults(
                search: search,
                isImporting: false,
                libraryStatuses: [:],
                selectedReleaseId: nil,
                loadingPick: nil,
                onRetry: {},
                onSelect: { _ in }
            )
            .frame(width: size.width, height: size.height)
            .importPreviewEnvironment(),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            return SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSProgressIndicator }
                .count
        }
    }
}
