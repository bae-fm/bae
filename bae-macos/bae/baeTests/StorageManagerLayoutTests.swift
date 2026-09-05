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

    func testStorageManagerShowsReleaseAndTotalUploadRates() async throws {
        let size = NSSize(width: 940, height: 600)
        let (_, host) = SnapshotTestSupport.hostInWindow(
            StorageManagerPreviewScene()
                .frame(width: size.width, height: size.height),
            size: size
        )

        try await settle(host)
        let observation = try XCTUnwrap(
            PreviewData.outboxStore()
                .storageUploadObservation(
                    forRelease: PreviewData.uploadGroup.releaseId
                )
        )
        XCTAssertEqual(
            observation.throughputText,
            QueueSummary.throughputText(bytesPerSecond: 3_200_000)
        )
        XCTAssertEqual(
            PreviewData.outboxSnapshot().throughputText,
            QueueSummary.throughputText(bytesPerSecond: 6_800_000)
        )
        let descendants = SnapshotTestSupport.descendants(of: host)
        XCTAssertTrue(descendants.contains { $0 is ProgressTrackNSView })
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

    func testInspectorFilesStartAtTopWithoutTabsOrTitleBar() async throws {
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
        let descendants = SnapshotTestSupport.descendants(of: host)
        XCTAssertFalse(
            descendants.compactMap { $0 as? NSSegmentedControl }
                .contains { $0.segmentCount == 2 }
        )
        let fileList = try XCTUnwrap(
            descendants.compactMap { $0 as? NSScrollView }
                .last { $0.documentView is NSTableView }
        )
        let frame = fileList.convert(fileList.bounds, to: host)
        XCTAssertGreaterThan(frame.minX, 400)
        XCTAssertEqual(frame.minY, host.bounds.minY, accuracy: 2)
        XCTAssertEqual(frame.height, host.bounds.height, accuracy: 2)
        withExtendedLifetime(window) {}
    }

    func testInspectorSplitOccupiesFullWindowHeight() async throws {
        let size = NSSize(width: 940, height: 600)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            StorageManagerPreviewScene(
                selectedReleaseId: "rel-row-1",
                inspectorPresented: true
            )
            .frame(width: size.width, height: size.height),
            size: size
        )

        try await settle(host)
        let splitView = try XCTUnwrap(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSSplitView }
                .first
        )
        let splitFrame = splitView.convert(splitView.bounds, to: host)

        XCTAssertEqual(splitFrame.height, host.bounds.height, accuracy: 1)
        withExtendedLifetime(window) {}
    }

    func testInspectorShowsFileProgressAlongsideContents() async throws {
        let size = NSSize(width: 940, height: 700)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            StorageManagerPreviewScene(
                selectedReleaseId: "rel-row-1",
                inspectorPresented: true,
                downloadSnapshot: PreviewData.emptyDownloadSnapshot,
                outputSnapshot: PreviewData.emptyOutputSnapshot
            )
            .frame(width: size.width, height: size.height),
            size: size
        )
        window.appearance = NSAppearance(named: .darkAqua)
        try await settle(host)

        let descendants = SnapshotTestSupport.descendants(of: host)
        let fileList = try XCTUnwrap(
            descendants.compactMap { $0 as? NSScrollView }
                .last { $0.documentView is NSTableView }
        )
        let files = try XCTUnwrap(fileList.documentView as? NSTableView)
        // Two contents files, one of which also uploads, and the remaining
        // five uploads (including generated artwork) each occupy one row.
        XCTAssertEqual(files.numberOfRows, 7)
        XCTAssertFalse(
            descendants.compactMap { $0 as? NSSegmentedControl }
                .contains { $0.segmentCount == 2 }
        )
        XCTAssertGreaterThanOrEqual(
            SnapshotTestSupport.descendants(of: fileList)
                .compactMap { $0 as? ProgressTrackNSView }.count,
            2
        )
        let screenshot = try await SnapshotTestSupport.capturePNG(
            host,
            size: size
        )
        let attachment = XCTAttachment(
            data: screenshot,
            uniformTypeIdentifier: "public.png"
        )
        attachment.name = "Storage inspector with file progress"
        attachment.lifetime = .keepAlways
        add(attachment)
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

extension StorageManagerLayoutTests {
    func testInspectorOpensWithoutSelection() async throws {
        let size = NSSize(width: 940, height: 600)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            StorageManagerPreviewScene(inspectorPresented: true)
                .frame(width: size.width, height: size.height),
            size: size
        )

        try await settle(host)
        let tableScrollView = try XCTUnwrap(storageTable(in: host))

        XCTAssertLessThan(tableScrollView.frame.width, size.width - 100)
        withExtendedLifetime(window) {}
    }

    func testClearingSelectionKeepsInspectorOpen() async throws {
        let size = NSSize(width: 940, height: 600)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            StorageManagerPreviewScene(
                selectedReleaseId: "rel-row-1",
                inspectorPresented: true
            )
            .frame(width: size.width, height: size.height),
            size: size
        )

        try await settle(host)
        let tableScrollView = try XCTUnwrap(storageTable(in: host))
        let tableView = try XCTUnwrap(
            tableScrollView.documentView as? NSTableView
        )
        let widthWithSelection = tableScrollView.frame.width

        tableView.deselectAll(nil)
        try await settle(host)

        XCTAssertEqual(
            tableScrollView.frame.width,
            widthWithSelection,
            accuracy: 1
        )
        withExtendedLifetime(window) {}
    }

    func testDoubleClickingReleaseTogglesInspector() async throws {
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
        let action = try XCTUnwrap(tableView.doubleAction)
        let widthBeforeOpening = tableScrollView.frame.width

        tableView.selectRowIndexes(
            IndexSet(integer: 0),
            byExtendingSelection: false
        )
        XCTAssertTrue(
            NSApp.sendAction(action, to: tableView.target, from: tableView)
        )
        try await settle(host)

        XCTAssertLessThan(
            tableScrollView.frame.width,
            widthBeforeOpening - 100
        )

        XCTAssertTrue(
            NSApp.sendAction(action, to: tableView.target, from: tableView)
        )
        try await settle(host)
        XCTAssertEqual(
            tableScrollView.frame.width,
            widthBeforeOpening,
            accuracy: 1
        )
        withExtendedLifetime(window) {}
    }

    func testReleaseContextMenuOffersInspect() async throws {
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
        let rowFrame = tableView.rect(ofRow: 0)
        let pointInWindow = tableView.convert(
            NSPoint(x: rowFrame.midX, y: rowFrame.midY),
            to: nil
        )
        let event = try XCTUnwrap(
            NSEvent.mouseEvent(
                with: .rightMouseDown,
                location: pointInWindow,
                modifierFlags: [],
                timestamp: 0,
                windowNumber: window.windowNumber,
                context: nil,
                eventNumber: 1,
                clickCount: 1,
                pressure: 1
            )
        )
        let menu = try XCTUnwrap(tableView.menu(for: event))
        menu.delegate?.menuNeedsUpdate?(menu)

        XCTAssertTrue(
            menu.items.contains { $0.title == String(localized: "Inspect") }
        )
        withExtendedLifetime(window) {}
    }
}
