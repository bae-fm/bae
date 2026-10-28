import AppKit
import BaeKit
import Testing

@testable import bae

private func cloudUploadReceipt(_ releaseIds: [String]) -> BridgeUploadReceipt {
    BridgeUploadReceipt(releaseIds: releaseIds)
}

@Suite("OutboxStore")
@MainActor
struct OutboxStorePauseTests {
    @Test("the pause control reads the pause each delivery carries")
    func uploadsPausedFollowsEachDelivery() {
        let store = OutboxStore(uploadsPaused: false)
        store.applyUploadsPaused(true)
        #expect(store.uploadsPaused)
        store.applyUploadsPaused(false)
        #expect(!store.uploadsPaused)
    }
}

@Suite("OutboxStore transfers")
@MainActor
struct OutboxStoreTransferTests {
    private func releaseTransfer(
        _ bytesDone: UInt64,
        _ throughputBps: UInt64
    ) -> BridgeReleaseUploadTransfer {
        BridgeReleaseUploadTransfer(
            bar: BridgeUploadBar(bytesDone: bytesDone, bytesTotal: 100),
            throughputBps: throughputBps
        )
    }

    /// A native view following one release's upload is shown what the
    /// stream holds for that release, and nothing for a release it holds
    /// nothing for.
    @Test("a release's view is shown that release's transfer")
    func releaseViewIsShownItsTransfer() {
        let store = OutboxStore(
            uploadsPaused: false,
            releaseProgress: .releaseUploads(
                holding: ["release-a": releaseTransfer(20, 10)]
            )
        )
        let view = NSView()
        var shown: [BridgeReleaseUploadTransfer?] = []
        let following = FrameFollowing<BridgeReleaseUploadTransfer>(
            view: view
        ) { shown.append($0) }

        following.follow(store.uploadProgress(forRelease: "release-a"))
        following.follow(store.uploadProgress(forRelease: "release-a"))
        following.follow(store.uploadProgress(forRelease: "release-b"))
        following.stop()

        #expect(shown == [releaseTransfer(20, 10), nil])
    }

    /// A file's view and the total's view each read their own stream.
    @Test("a file's view and the total's view read their own streams")
    func fileAndTotalViewsReadTheirStreams() {
        let total = BridgeUploadTransfer(
            bar: BridgeUploadBar(bytesDone: 10, bytesTotal: 100),
            throughputBps: 5
        )
        let store = OutboxStore(
            uploadsPaused: false,
            fileShares: .fileShares(holding: ["file-a": 0.25]),
            filesBeingSent: .uploadTotal(holding: total)
        )
        let view = NSView()
        var shares: [Double?] = []
        var totals: [BridgeUploadTransfer?] = []
        let share = FrameFollowing<Double>(view: view) { shares.append($0) }
        let sent = FrameFollowing<BridgeUploadTransfer>(view: view) {
            totals.append($0)
        }

        share.follow(store.fileShare(forFile: "file-a"))
        sent.follow(store.totalProgress)
        share.stop()
        sent.stop()

        #expect(shares == [0.25])
        #expect(totals == [total])
    }
}
