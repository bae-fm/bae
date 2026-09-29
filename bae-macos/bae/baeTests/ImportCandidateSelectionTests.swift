import AppKit
import BaeKit
import SwiftUI
import Testing
import XCTest

@testable import bae

@Suite("Import candidate selection")
struct ImportCandidateSelectionTests {
    @MainActor
    @Test("folder scan activity renders an indeterminate progress control")
    func folderScanActivityRendersIndeterminateProgress() async throws {
        let size = NSSize(width: 180, height: 40)
        try await SnapshotTestSupport.withHostedWindow(
            FolderScanProgressIndicator(
                activity: BridgeFolderScanActivity(
                    foundCount: 179,
                    folders: [
                        BridgeActiveFolderScan(
                            watchedFolderPath: "/imports/incoming",
                            watchedFolderName: "Incoming",
                            foundCount: 179
                        )
                    ]
                )
            )
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            host.layoutSubtreeIfNeeded()
            let progress = try #require(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSProgressIndicator }
                    .first
            )
            #expect(progress.isIndeterminate)
        }
    }

    @MainActor
    @Test("a row renders without resolving the outbox environment")
    func rowRendersFromSuppliedUploadPresentation() async throws {
        let size = NSSize(width: 400, height: 80)
        try await SnapshotTestSupport.withHostedWindow(
            TriageRowView(
                row: PreviewData.triageRowDoneImported,
                coverContent: nil,
                isGroupMember: false
            )
            .environment(ImageStore.stub())
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            host.layoutSubtreeIfNeeded()
            #expect(host.fittingSize.height > 0)
        }
    }

    /// What is running for a candidate reaches its row with the row: the
    /// list delivers the row again when it changes, and the row draws what
    /// the row says — a run going draws the spinner, and none draws none.
    @MainActor
    @Test("a row draws the run its own row reports")
    func rowDrawsItsRowsLiveState() async throws {
        let idle = PreviewData.triageRowUnidentified
        var running = idle
        running.live = BridgeCandidateLiveState(
            identification: .running,
            import: nil,
            actions: [.cancelIdentification, .skip, .revealFolder],
            standing: .identifying
        )
        func spinners(_ row: BridgeTriageRow) async throws -> Int {
            let size = NSSize(width: 400, height: 80)
            return try await SnapshotTestSupport.withHostedWindow(
                TriageRowView(
                    row: row,
                    coverContent: nil,
                    isGroupMember: false
                )
                .environment(ImageStore.stub())
                .frame(width: size.width, height: size.height),
                size: size
            ) { _, host in
                try await SnapshotTestSupport.settle(host)
                return SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSProgressIndicator }
                    .count
            }
        }

        #expect(try await spinners(idle) == 0)
        #expect(try await spinners(running) == 1)
    }
}

final class PopoverAnimationTests: XCTestCase {
    @MainActor
    func testPopoverBehaviorDisablesEnclosingPopoverAnimation() async throws {
        let size = NSSize(width: 80, height: 40)
        try await SnapshotTestSupport.withHostedWindow(
            Color.clear.frame(width: size.width, height: size.height),
            size: size
        ) { window, anchor in
            let popover = NSPopover()
            popover.animates = true
            let contentViewController = NSHostingController(
                rootView: PopoverBehavior()
                    .frame(width: 120, height: 80)
            )
            popover.contentViewController = contentViewController
            // A popover is placed on a display whatever its anchor's window
            // is, so the one this test opens is shown transparent to the eye
            // and to the pointer; the test reads the popover, not its pixels.
            let observer = NotificationCenter.default.addObserver(
                forName: NSPopover.willShowNotification,
                object: popover,
                queue: nil
            ) { _ in
                MainActor.assumeIsolated {
                    let window = contentViewController.view.window
                    window?.alphaValue = 0
                    window?.ignoresMouseEvents = true
                }
            }
            defer { NotificationCenter.default.removeObserver(observer) }
            popover.show(
                relativeTo: anchor.bounds,
                of: anchor,
                preferredEdge: .maxY
            )

            try await SnapshotTestSupport.settle(contentViewController.view)

            XCTAssertFalse(popover.animates)
            popover.performClose(nil)
        }
    }
}
