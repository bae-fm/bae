import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The band draws the title search only for a run that searches by title.
@MainActor
@Suite("Identifier band title search")
struct IdentifierBandTitleSearchTests {
    private func fields(of search: BridgeSearchStep) async throws -> [NSView] {
        var run = PreviewData.identifyRunFound
        run.search = search
        let size = NSSize(width: 600, height: 200)
        var found: [NSView] = []
        try await SnapshotTestSupport.withHostedWindow(
            MacIdentifierBand(
                run: run,
                onToggleLookup: { _ in },
                onToggleCatalogAgreement: { _ in },
                onRetryFailed: {},
                onEditTitleSearch: { _, _ in }
            )
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            found = SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSTextField }
        }
        return found
    }

    @Test(
        "a run with no title to search by draws no empty Artist and Title fields"
    )
    func noTitleDrawsNoFields() async throws {
        #expect(try await fields(of: .noTitle).isEmpty)
    }
}
