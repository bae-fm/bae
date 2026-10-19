import AppKit
import BaeKit
import SwiftUI
import Testing

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
            standing: .identifying,
            badge: nil
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
