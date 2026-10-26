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
            fraction: Double(bytesDone) / 100
        )
    }

    @Test("a release's progress follows each change that names it")
    func releaseProgressFollowsItsChanges() {
        let store = DownloadStore(snapshot: PreviewData.emptyDownloadSnapshot)
        store.applyTransferChange(
            .reset(transfers: [
                BridgeKeyedDownloadTransfer(
                    releaseId: "release-a",
                    progress: progress(10)
                )
            ])
        )
        #expect(store.progress(forRelease: "release-a") == progress(10))

        store.applyTransferChange(
            .updated(releaseId: "release-a", progress: progress(40))
        )
        #expect(store.progress(forRelease: "release-a") == progress(40))

        store.applyTransferChange(.removed(releaseId: "release-a"))
        #expect(store.progress(forRelease: "release-a") == nil)
    }
}
