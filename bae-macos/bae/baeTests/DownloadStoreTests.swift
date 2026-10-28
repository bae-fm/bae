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

    @Test("a release's progress follows each change that names it")
    func releaseProgressFollowsItsChanges() throws {
        let store = DownloadStore()
        store.applyTransferChange(
            .reset(transfers: [
                BridgeKeyedDownloadTransfer(
                    releaseId: "release-a",
                    progress: progress(10)
                )
            ])
        )
        let value = try #require(store.transfer(forRelease: "release-a"))
        #expect(value.progress == progress(10))

        store.applyTransferChange(
            .updated(releaseId: "release-a", progress: progress(40))
        )
        #expect(
            store.transfer(forRelease: "release-a") === value,
            "the release keeps its one observed value"
        )
        #expect(value.progress == progress(40))

        store.applyTransferChange(.removed(releaseId: "release-a"))
        #expect(store.transfer(forRelease: "release-a") == nil)
    }
}
