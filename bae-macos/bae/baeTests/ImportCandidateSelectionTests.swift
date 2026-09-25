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
    func folderScanActivityRendersIndeterminateProgress() throws {
        let size = NSSize(width: 180, height: 40)
        let (_, host) = SnapshotTestSupport.hostInWindow(
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
        )

        host.layoutSubtreeIfNeeded()
        let progress = try #require(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSProgressIndicator }
                .first
        )
        #expect(progress.isIndeterminate)
    }

    @MainActor
    @Test("a row renders without resolving the outbox environment")
    func rowRendersFromSuppliedUploadPresentation() {
        let size = NSSize(width: 400, height: 80)
        let (_, host) = SnapshotTestSupport.hostInWindow(
            TriageRowView(
                row: PreviewData.triageRowDoneImported,
                coverContent: nil,
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            .environment(ImageStore.stub())
            .frame(width: size.width, height: size.height),
            size: size
        )

        host.layoutSubtreeIfNeeded()
        #expect(host.fittingSize.height > 0)
    }

    /// What is running for a candidate reaches its row through the row's own
    /// subscription, asked with the row's basis: a run the subscription says
    /// is going draws the row's spinner, and the list's row alone draws none.
    @MainActor
    @Test("a row draws the run its own subscription reports")
    func rowDrawsItsOwnLiveState() async throws {
        let row = PreviewData.triageRowUnidentified
        let asked = RecordedLiveStateSubscription()
        let running = Importer(
            subscribeCandidateLiveState: { key, basis, callback in
                asked.record(key: key, basis: basis)
                callback.onValue(
                    value: BridgeCandidateLiveState(
                        identification: .running,
                        importing: false,
                        actions: [.skip]
                    )
                )
                return asked
            }
        )
        func spinners(_ importer: Importer) async -> Int {
            let size = NSSize(width: 400, height: 80)
            let (_, host) = SnapshotTestSupport.hostInWindow(
                TriageRowView(
                    row: row,
                    coverContent: nil,
                    isGroupMember: false,
                    onReveal: {},
                    onSkip: { _ in }
                )
                .environment(importer)
                .environment(ImageStore.stub())
                .frame(width: size.width, height: size.height),
                size: size
            )
            for _ in 0..<50 {
                await Task.yield()
                host.layoutSubtreeIfNeeded()
            }
            return SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSProgressIndicator }
                .count
        }

        #expect(await spinners(Importer()) == 0)
        #expect(await spinners(running) == 1)
        #expect(asked.keys == [row.candidateKey])
        #expect(asked.bases == [row.actionBasis])
    }

    @MainActor
    @Test("native row selection is the bulk-action selection")
    func candidateCanBeSelected() async throws {
        try await assertCandidateCanBeSelected(isGroupMember: false)
    }

    @MainActor
    @Test("native group-member selection is the bulk-action selection")
    func groupedCandidateCanBeSelected() async throws {
        try await assertCandidateCanBeSelected(isGroupMember: true)
    }

    @MainActor
    private func assertCandidateCanBeSelected(
        isGroupMember: Bool
    ) async throws {
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.pending)
        let store = PreviewData.importTabScene().store
        let slot = ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: [
                .candidate(
                    stableKey:
                        "candidate:\(PreviewData.triageRowReady.candidateKey)",
                    row: PreviewData.triageRowReady,
                    isGroupMember: isGroupMember
                )
            ]
        )
        let size = NSSize(width: 400, height: 320)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportCandidateListContent(
                importStore: store,
                listSlot: slot,
                selectedKeys: Binding(
                    get: { uiStore.selectedFolderCandidates },
                    set: { uiStore.setFolderCandidateSelection($0) }
                ),
                onAddFolder: {},
                onRemoveFolder: { _ in },
                onRefreshFolder: { _ in },
                onCombineFolder: { _ in },
                onSeparate: { _ in },
                onSkip: { _, _ in },
                onReveal: { _ in }
            )
            .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
            .environment(uiStore)
            .environment(ImageStore.stub())
            .frame(width: size.width, height: size.height),
            size: size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)

        // Exercise the native list selection, not a second checkbox state.
        let tableView = try #require(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTableView }
                .first
        )
        tableView.selectRowIndexes(
            IndexSet(integer: 0),
            byExtendingSelection: false
        )
        await SnapshotTestSupport.settle(host)

        #expect(
            uiStore.selectedFolderCandidates
                == [PreviewData.triageRowReady.candidateKey]
        )
    }

}

final class PopoverAnimationTests: XCTestCase {
    @MainActor
    func testPopoverBehaviorDisablesEnclosingPopoverAnimation() async {
        let size = NSSize(width: 80, height: 40)
        let (window, anchor) = SnapshotTestSupport.hostInWindow(
            Color.clear.frame(width: size.width, height: size.height),
            size: size
        )
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

        await SnapshotTestSupport.settle(contentViewController.view)

        XCTAssertFalse(popover.animates)
        popover.performClose(nil)
        withExtendedLifetime(window) {}
    }
}

/// Records what a row asked its live-state subscription for.
private final class RecordedLiveStateSubscription: LiveSubscriptionProtocol,
    @unchecked Sendable
{
    private let lock = NSLock()
    private var asked: [(String, BridgeCandidateActionBasis)] = []

    var keys: [String] { lock.withLock { asked.map(\.0) } }
    var bases: [BridgeCandidateActionBasis] { lock.withLock { asked.map(\.1) } }

    func record(key: String, basis: BridgeCandidateActionBasis) {
        lock.withLock { asked.append((key, basis)) }
    }

    func cancel() {}
}
