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

    func testSelectingReleaseWithoutTransferKeepsAvailableHeight() async throws
    {
        let size = NSSize(width: 700, height: 400)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            StorageManagerPreviewScene(
                downloadSnapshot: PreviewData.emptyDownloadSnapshot,
                outputSnapshot: PreviewData.emptyOutputSnapshot,
                outboxSnapshot: PreviewData.outboxSnapshot(
                    uploadGroups: [],
                    deletes: []
                )
            )
            .frame(width: size.width, height: size.height),
            size: size
        )

        try await settle(host)
        let tableScrollView = try XCTUnwrap(storageTable(in: host))
        let tableView = try XCTUnwrap(
            tableScrollView.documentView as? NSTableView
        )
        let heightBeforeSelection = tableScrollView.frame.height

        tableView.selectRowIndexes(
            IndexSet(integer: 3),
            byExtendingSelection: false
        )
        try await settle(host)

        let heightAfterSelection = tableScrollView.frame.height
        XCTAssertGreaterThanOrEqual(
            heightAfterSelection,
            heightBeforeSelection - 1
        )
        withExtendedLifetime(window) {}
    }

    func testSelectingReleaseDoesNotPresentTransferInspector() async throws {
        let size = NSSize(width: 940, height: 600)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            StorageManagerPreviewScene()
                .frame(width: size.width, height: size.height),
            size: size
        )

        try await settle(host)
        let tableScrollView = try XCTUnwrap(storageTable(in: host))
        let tableView = try XCTUnwrap(
            tableScrollView.documentView as? NSTableView
        )
        let widthBeforeSelection = tableScrollView.frame.width

        tableView.selectRowIndexes(
            IndexSet(integer: 0),
            byExtendingSelection: false
        )
        try await settle(host)

        XCTAssertEqual(
            tableScrollView.frame.width,
            widthBeforeSelection,
            accuracy: 1
        )
        withExtendedLifetime(window) {}
    }

    func testReleaseListUsesFlatTable() async throws {
        let size = NSSize(width: 940, height: 600)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            StorageManagerPreviewScene()
                .frame(width: size.width, height: size.height),
            size: size
        )

        try await settle(host)
        let tableScrollView = try XCTUnwrap(storageTable(in: host))
        let tableView = try XCTUnwrap(
            tableScrollView.documentView as? NSTableView
        )

        XCTAssert(type(of: tableView) == NSTableView.self)
        withExtendedLifetime(window) {}
    }

    func testOpenInspectorHeaderStaysAtTopOfStorageContent() async throws {
        let size = NSSize(width: 1_440, height: 900)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            StorageManagerPreviewScene(
                selectedReleaseId: "rel-row-4",
                inspectorPresented: true,
                downloadSnapshot: PreviewData.emptyDownloadSnapshot,
                outputSnapshot: PreviewData.emptyOutputSnapshot,
                outboxSnapshot: PreviewData.outboxSnapshot(
                    uploadGroups: [],
                    deletes: []
                )
            )
            .frame(width: size.width, height: size.height),
            size: size
        )

        try await settle(host)
        let tableScrollView = try XCTUnwrap(storageTable(in: host))
        let inspectorPicker = try XCTUnwrap(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSSegmentedControl }
                .first { $0.segmentCount == 2 }
        )
        let tableFrame = tableScrollView.convert(
            tableScrollView.bounds,
            to: host
        )
        let pickerFrame = inspectorPicker.convert(
            inspectorPicker.bounds,
            to: host
        )

        XCTAssertLessThan(pickerFrame.midY, tableFrame.minY + 100)
        withExtendedLifetime(window) {}
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
        let tableView = try XCTUnwrap(
            scrollView.documentView as? NSTableView
        )

        XCTAssertEqual(
            tableView.columnAutoresizingStyle,
            .firstColumnOnlyAutoresizingStyle
        )
        XCTAssertTrue(scrollView.hasHorizontalScroller)
    }

    private func storageTable(in host: NSView) -> NSScrollView? {
        SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSScrollView }
            .first { $0.documentView is NSTableView }
    }

    private func settle(_ host: NSView) async throws {
        host.layoutSubtreeIfNeeded()
        try await Task.sleep(for: .milliseconds(500))
        host.layoutSubtreeIfNeeded()
    }
}
