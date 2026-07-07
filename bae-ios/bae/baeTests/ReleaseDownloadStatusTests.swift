import Testing

@testable import bae

/// `ReleaseDownloadStatus` is the album-detail download control's state, joined
/// from the release summary (`pinned` / `storageActions`) and this release's
/// download-queue entry. These cover the join's precedence — a live queue entry
/// outranks `pinned`, `pinned` outranks an available `.pin` action, and no
/// cloud home yields no control at all — plus the snapshot lookup that feeds it.
/// Pure derivation only; the store-application side needs a live library, which
/// iOS unit tests don't construct.
@Suite("ReleaseDownloadStatus derivation")
struct ReleaseDownloadStatusTests {
    @Test("pinned with no queue entry is Downloaded")
    func pinnedIsDownloaded() {
        let status = ReleaseDownloadStatus(
            pinned: true,
            storageActions: [.unpin, .makeLocal],
            queueState: nil
        )
        #expect(status == .downloaded)
    }

    @Test("a queued entry wins even when a pin action is available")
    func queuedWinsOverPinAction() {
        let status = ReleaseDownloadStatus(
            pinned: false,
            storageActions: [.pin],
            queueState: .queued
        )
        #expect(status == .queued)
    }

    @Test("an active entry carries its exact progress")
    func activeCarriesProgress() {
        let progress = BridgeDownloadTransferProgress(
            bytesDone: 1_024,
            bytesTotal: 4_096,
            fraction: 0.25
        )
        let status = ReleaseDownloadStatus(
            pinned: false,
            storageActions: [],
            queueState: .active(progress: progress)
        )
        #expect(status == .downloading(progress))
    }

    @Test("a failed entry carries its error message")
    func failedCarriesMessage() {
        let status = ReleaseDownloadStatus(
            pinned: false,
            storageActions: [.pin],
            queueState: .failed(error: "read failed")
        )
        #expect(status == .failed("read failed"))
    }

    @Test("a queue entry outranks pinned in the post-success race window")
    func queueEntryOutranksPinned() {
        let progress = BridgeDownloadTransferProgress(
            bytesDone: 4_096,
            bytesTotal: 4_096,
            fraction: 1.0
        )
        let status = ReleaseDownloadStatus(
            pinned: true,
            storageActions: [],
            queueState: .active(progress: progress)
        )
        #expect(status == .downloading(progress))
    }

    @Test("not pinned with a pin action is Available; desktop actions ignored")
    func pinActionIsAvailable() {
        let status = ReleaseDownloadStatus(
            pinned: false,
            storageActions: [.pin, .makeLocal],
            queueState: nil
        )
        #expect(status == .available)
    }

    @Test("no cloud home (no actions) yields no control")
    func noActionsYieldsNil() {
        let status = ReleaseDownloadStatus(
            pinned: false,
            storageActions: [],
            queueState: nil
        )
        #expect(status == nil)
    }

    @Test("unpin action without a pinned flag yields no control")
    func unpinActionWithoutPinnedYieldsNil() {
        // State is driven by `pinned`, not the action list: without `pinned`
        // there is nothing downloaded to remove and no pin offered.
        let status = ReleaseDownloadStatus(
            pinned: false,
            storageActions: [.unpin, .makeLocal],
            queueState: nil
        )
        #expect(status == nil)
    }
}

@Suite("BridgeDownloadSnapshot release lookup")
struct DownloadSnapshotLookupTests {
    private func op(
        _ releaseId: String,
        _ state: BridgeDownloadState
    ) -> BridgeDownloadOp {
        BridgeDownloadOp(
            releaseId: releaseId,
            title: "Album Title",
            fileCount: 10,
            totalSize: 1_000,
            createdAt: 0,
            state: state
        )
    }

    private func snapshot(_ ops: [BridgeDownloadOp]) -> BridgeDownloadSnapshot {
        BridgeDownloadSnapshot(
            downloads: ops,
            total: BridgeDownloadProgress(queued: 0, active: 0, failed: 0),
            paused: false
        )
    }

    @Test("finds the matching release's queue state")
    func findsMatchingState() {
        let snap = snapshot([op("rel-1", .queued)])
        #expect(snap.state(forRelease: "rel-1") == .queued)
    }

    @Test("an absent release has no queue state")
    func absentReleaseIsNil() {
        let snap = snapshot([op("rel-1", .queued)])
        #expect(snap.state(forRelease: "rel-2") == nil)
    }

    @Test("picks the right entry among several")
    func picksRightEntryAmongSeveral() {
        let progress = BridgeDownloadTransferProgress(
            bytesDone: 2,
            bytesTotal: 8,
            fraction: 0.25
        )
        let snap = snapshot([
            op("rel-1", .queued),
            op("rel-2", .active(progress: progress)),
            op("rel-3", .failed(error: "read failed")),
        ])
        #expect(snap.state(forRelease: "rel-2") == .active(progress: progress))
        #expect(snap.state(forRelease: "rel-3") == .failed(error: "read failed"))
    }
}
