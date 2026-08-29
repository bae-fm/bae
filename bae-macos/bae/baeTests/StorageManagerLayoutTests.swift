import AppKit
import BaeKit
import SwiftUI
import XCTest

@testable import bae

@MainActor
final class StorageManagerLayoutTests: XCTestCase {
    func testQueueMessagesResolveFromBaeKitCatalog() {
        XCTAssertEqual(
            QueueSummary.message("core.outbox.publishing"),
            "%lld publishing"
        )
    }

    func testCompactActivityLeavesReleaseTableUsable() async throws {
        let size = NSSize(width: 700, height: 400)
        let (_, host) = SnapshotTestSupport.hostInWindow(
            StorageManagerPreviewScene(selectedReleaseId: "rel-row-1")
                .frame(width: size.width, height: size.height),
            size: size
        )

        try await settle(host)
        let tableScrollView = try XCTUnwrap(storageTable(in: host))

        XCTAssertGreaterThanOrEqual(tableScrollView.frame.height, 220)
    }

    func testTableUsesIntentionalColumnSizing() async throws {
        let size = NSSize(width: 1_440, height: 900)
        let (_, host) = SnapshotTestSupport.hostInWindow(
            StorageManagerPreviewScene()
                .frame(width: size.width, height: size.height),
            size: size
        )

        try await settle(host)
        let scrollView = try XCTUnwrap(storageTable(in: host))
        let outlineView = try XCTUnwrap(
            scrollView.documentView as? NSOutlineView
        )

        XCTAssertEqual(
            outlineView.columnAutoresizingStyle,
            .firstColumnOnlyAutoresizingStyle
        )
        XCTAssertTrue(scrollView.hasHorizontalScroller)
    }

    private func storageTable(in host: NSView) -> NSScrollView? {
        SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSScrollView }
            .first { $0.documentView is NSOutlineView }
    }

    private func settle(_ host: NSView) async throws {
        host.layoutSubtreeIfNeeded()
        try await Task.sleep(for: .milliseconds(500))
        host.layoutSubtreeIfNeeded()
    }
}
