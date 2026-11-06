import AppKit
import BaeKit
import SwiftUI
import XCTest

@testable import bae

@MainActor
final class StorageManagerLayoutTests: XCTestCase {
    func testQueueMessagesResolveFromBaeKitCatalog() {
        XCTAssertEqual(
            localizedCoreString("core.queue.uploading"),
            "%lld uploading"
        )
    }

    func testCompactActivityLeavesReleaseTableUsable() async throws {
        let size = NSSize(width: 700, height: 400)
        let scene = StorageManagerPreviewScene(selectedReleaseId: "rel-row-1")
        try await SnapshotTestSupport.withHostedWindow(
            scene
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            try await settleFirstPage(of: scene, in: host)
            let tableScrollView = try XCTUnwrap(storageTable(in: host))

            XCTAssertGreaterThanOrEqual(tableScrollView.frame.height, 220)
        }
    }

    /// The rates stay the outbox's to report; the table draws an upload in
    /// its row's chip, with no bar or rate beside it.
    func testStorageTableDrawsUploadsInTheirChipsWithoutBars() async throws {
        let size = NSSize(width: 940, height: 600)
        let scene = StorageManagerPreviewScene()
        try await SnapshotTestSupport.withHostedWindow(
            scene
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            try await settleFirstPage(of: scene, in: host)
            let row = try XCTUnwrap(
                PreviewData.storageRows(
                    PreviewData.storageRows,
                    uploading: PreviewData.defaultUploads
                )
                .first {
                    $0.release.id == PreviewData.uploadingRelease.releaseId
                }
            )
            // The scene's outbox moves this row's release and the total.
            XCTAssertEqual(
                PreviewData.uploadTransfers[row.release.id]?.throughputBps,
                3_200_000
            )
            XCTAssertEqual(PreviewData.uploadTotal.throughputBps, 6_800_000)
            let descendants = SnapshotTestSupport.descendants(of: host)
            XCTAssertFalse(descendants.contains { $0 is ProgressTrackNSView })
        }
    }

    func testSelectingReleaseWithoutTransferKeepsAvailableHeight() async throws
    {
        let size = NSSize(width: 700, height: 400)
        let scene = StorageManagerPreviewScene(
            downloads: [],
            outputs: [],
            uploads: []
        )
        try await SnapshotTestSupport.withHostedWindow(
            scene
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            try await settleFirstPage(of: scene, in: host)
            let tableScrollView = try XCTUnwrap(storageTable(in: host))
            let tableView = try XCTUnwrap(
                tableScrollView.documentView as? NSTableView
            )
            let heightBeforeSelection = tableScrollView.frame.height

            tableView.selectRowIndexes(
                IndexSet(integer: 3),
                byExtendingSelection: false
            )
            try await SnapshotTestSupport.settle(host, answering: scene.answers)

            let heightAfterSelection = tableScrollView.frame.height
            XCTAssertGreaterThanOrEqual(
                heightAfterSelection,
                heightBeforeSelection - 1
            )
        }
    }

    func testSelectingReleaseDoesNotPresentTransferInspector() async throws {
        let size = NSSize(width: 940, height: 600)
        let scene = StorageManagerPreviewScene()
        try await SnapshotTestSupport.withHostedWindow(
            scene
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            try await settleFirstPage(of: scene, in: host)
            let tableScrollView = try XCTUnwrap(storageTable(in: host))
            let tableView = try XCTUnwrap(
                tableScrollView.documentView as? NSTableView
            )
            let widthBeforeSelection = tableScrollView.frame.width

            tableView.selectRowIndexes(
                IndexSet(integer: 0),
                byExtendingSelection: false
            )
            try await SnapshotTestSupport.settle(host, answering: scene.answers)

            XCTAssertEqual(
                tableScrollView.frame.width,
                widthBeforeSelection,
                accuracy: 1
            )
        }
    }

    func testReleaseListUsesFlatTable() async throws {
        let size = NSSize(width: 940, height: 600)
        let scene = StorageManagerPreviewScene()
        try await SnapshotTestSupport.withHostedWindow(
            scene
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            try await settleFirstPage(of: scene, in: host)
            let tableScrollView = try XCTUnwrap(storageTable(in: host))
            let tableView = try XCTUnwrap(
                tableScrollView.documentView as? NSTableView
            )

            XCTAssert(type(of: tableView) == NSTableView.self)
        }
    }

    func testInspectorFilesStartAtTopWithoutTabsOrTitleBar() async throws {
        let size = NSSize(width: 1_440, height: 900)
        let scene = StorageManagerPreviewScene(
            selectedReleaseId: "rel-row-4",
            inspectorPresented: true,
            downloads: [],
            outputs: [],
            uploads: []
        )
        try await SnapshotTestSupport.withHostedWindow(
            scene
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            try await settleFirstPage(of: scene, in: host)
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
        }
    }

    func testInspectorSplitOccupiesFullWindowHeight() async throws {
        let size = NSSize(width: 940, height: 600)
        let scene = StorageManagerPreviewScene(
            selectedReleaseId: "rel-row-1",
            inspectorPresented: true
        )
        try await SnapshotTestSupport.withHostedWindow(
            scene
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            try await settleFirstPage(of: scene, in: host)
            let splitView = try XCTUnwrap(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSSplitView }
                    .first
            )
            let splitFrame = splitView.convert(splitView.bounds, to: host)

            XCTAssertEqual(splitFrame.height, host.bounds.height, accuracy: 1)
        }
    }

    /// The inspector lists the release's files one line each, its upload's
    /// progress drawn once above them and none on any row.
    func testInspectorShowsEachFileOnOneLine() async throws {
        let size = NSSize(width: 940, height: 700)
        let scene = StorageManagerPreviewScene(
            selectedReleaseId: "rel-row-1",
            inspectorPresented: true,
            downloads: [],
            outputs: []
        )
        try await SnapshotTestSupport.withHostedWindow(
            scene
                .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            window.appearance = NSAppearance(named: .darkAqua)
            try await settleFirstPage(of: scene, in: host)

            let descendants = SnapshotTestSupport.descendants(of: host)
            let fileList = try XCTUnwrap(
                descendants.compactMap { $0 as? NSScrollView }
                    .last { $0.documentView is NSTableView }
            )
            let files = try XCTUnwrap(fileList.documentView as? NSTableView)
            // The release's audio file and its cover.
            XCTAssertEqual(files.numberOfRows, 2)
            for row in 0..<files.numberOfRows {
                XCTAssertLessThan(files.rect(ofRow: row).height, 40)
            }
            XCTAssertFalse(
                descendants.compactMap { $0 as? NSSegmentedControl }
                    .contains { $0.segmentCount == 2 }
            )
            XCTAssertEqual(
                SnapshotTestSupport.descendants(of: fileList)
                    .compactMap { $0 as? ProgressTrackNSView }.count,
                0
            )
            XCTAssertEqual(
                descendants.compactMap { $0 as? ProgressTrackNSView }.count,
                1,
                "the release's upload summary draws the one bar"
            )
        }
    }

    func testTableUsesIntentionalColumnSizing() async throws {
        let size = NSSize(width: 1_440, height: 900)
        let scene = StorageManagerPreviewScene()
        try await SnapshotTestSupport.withHostedWindow(
            scene
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            try await settleFirstPage(of: scene, in: host)
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
    }

    /// Settle the manager once its list holds its first page. Its read is
    /// asked from the view's task, so until the list is there an idle tree
    /// with no read unanswered may be one that has not asked yet.
    private func settleFirstPage(
        of scene: StorageManagerPreviewScene,
        in host: NSView
    ) async throws {
        try await Wait.until { scene.store.list != nil }
        try await SnapshotTestSupport.settle(host, answering: scene.answers)
    }

    private func storageTable(in host: NSView) -> NSScrollView? {
        SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSScrollView }
            .first { $0.documentView is NSTableView }
    }

}

extension StorageManagerLayoutTests {
    func testInspectorOpensWithoutSelection() async throws {
        let size = NSSize(width: 940, height: 600)
        let scene = StorageManagerPreviewScene(inspectorPresented: true)
        try await SnapshotTestSupport.withHostedWindow(
            scene
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            try await settleFirstPage(of: scene, in: host)
            let tableScrollView = try XCTUnwrap(storageTable(in: host))

            XCTAssertLessThan(tableScrollView.frame.width, size.width - 100)
        }
    }

    func testClearingSelectionKeepsInspectorOpen() async throws {
        let size = NSSize(width: 940, height: 600)
        let scene = StorageManagerPreviewScene(
            selectedReleaseId: "rel-row-1",
            inspectorPresented: true
        )
        try await SnapshotTestSupport.withHostedWindow(
            scene
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            try await settleFirstPage(of: scene, in: host)
            let tableScrollView = try XCTUnwrap(storageTable(in: host))
            let tableView = try XCTUnwrap(
                tableScrollView.documentView as? NSTableView
            )
            let widthWithSelection = tableScrollView.frame.width

            tableView.deselectAll(nil)
            try await SnapshotTestSupport.settle(host, answering: scene.answers)

            XCTAssertEqual(
                tableScrollView.frame.width,
                widthWithSelection,
                accuracy: 1
            )
        }
    }

    func testDoubleClickingReleaseTogglesInspector() async throws {
        let size = NSSize(width: 940, height: 600)
        let scene = StorageManagerPreviewScene()
        try await SnapshotTestSupport.withHostedWindow(
            scene
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            try await settleFirstPage(of: scene, in: host)
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
            try await SnapshotTestSupport.settle(host, answering: scene.answers)

            XCTAssertLessThan(
                tableScrollView.frame.width,
                widthBeforeOpening - 100
            )

            XCTAssertTrue(
                NSApp.sendAction(action, to: tableView.target, from: tableView)
            )
            try await SnapshotTestSupport.settle(host, answering: scene.answers)
            XCTAssertEqual(
                tableScrollView.frame.width,
                widthBeforeOpening,
                accuracy: 1
            )
        }
    }

    func testReleaseContextMenuOffersInspect() async throws {
        let size = NSSize(width: 940, height: 600)
        let scene = StorageManagerPreviewScene()
        try await SnapshotTestSupport.withHostedWindow(
            scene
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            try await settleFirstPage(of: scene, in: host)
            let tableScrollView = try XCTUnwrap(storageTable(in: host))
            let tableView = try XCTUnwrap(
                tableScrollView.documentView as? NSTableView
            )
            let rowFrame = tableView.rect(ofRow: 0)
            let pointInWindow = tableView.convert(
                NSPoint(x: rowFrame.midX, y: rowFrame.midY),
                to: nil
            )
            let menu = try XCTUnwrap(
                HostedInput.contextMenu(at: pointInWindow, in: tableView)
            )
            menu.delegate?.menuNeedsUpdate?(menu)

            XCTAssertTrue(
                menu.items.contains { $0.title == String(localized: "Inspect") }
            )
        }
    }
}
