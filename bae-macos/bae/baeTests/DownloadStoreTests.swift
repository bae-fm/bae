import BaeKit
import Testing

@testable import bae

@Suite("DownloadStore transfers")
@MainActor
struct DownloadStoreTransferTests {
    private func progress(_ bytesDone: UInt64) -> BridgeDownloadTransferProgress
    {
        BridgeDownloadTransferProgress(
            bytesDone: bytesDone,
            bytesTotal: 100,
            fraction: Double(bytesDone) / 100,
            throughputBps: nil
        )
    }

    /// A release's latest progress is each change that names it, and none
    /// once the release stops downloading.
    @Test("a release's progress follows each change that names it")
    func releaseProgressFollowsItsChanges() {
        let store = DownloadStore()
        store.applyTransferChange(
            .reset(transfers: [
                BridgeKeyedDownloadTransfer(
                    releaseId: "release-a",
                    progress: progress(10)
                )
            ])
        )
        let source = store.downloadProgress(forRelease: "release-a")
        #expect(source.latest == progress(10))

        store.applyTransferChange(
            .updated(releaseId: "release-a", progress: progress(40))
        )
        store.applyTransferChange(
            .updated(releaseId: "release-b", progress: progress(70))
        )
        #expect(source.latest == progress(40))

        store.applyTransferChange(.removed(releaseId: "release-a"))
        #expect(source.latest == nil)
    }
}
