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

    /// A native view following a release hears its progress now, each
    /// change that names it, and none once the release stops downloading.
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
        let source = store.releaseTransfer(forRelease: "release-a")
        var heard: [BridgeDownloadTransferProgress?] = []
        let observation = source.values.observe(source.key) {
            heard.append($0)
        }

        store.applyTransferChange(
            .updated(releaseId: "release-a", progress: progress(40))
        )
        store.applyTransferChange(
            .updated(releaseId: "release-b", progress: progress(70))
        )
        store.applyTransferChange(.removed(releaseId: "release-a"))
        source.values.stopObserving(observation)
        store.applyTransferChange(
            .updated(releaseId: "release-a", progress: progress(90))
        )

        #expect(heard == [progress(10), progress(40), nil])
    }
}
