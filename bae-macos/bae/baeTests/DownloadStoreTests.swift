import AppKit
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

    /// A native view following one release's download is shown what the
    /// stream holds for that release, and nothing once it follows a release
    /// the stream holds nothing for.
    @Test("a release's view is shown that release's progress")
    func releaseViewIsShownItsProgress() {
        let store = DownloadStore(
            releaseProgress: .downloads(
                holding: ["release-a": progress(10), "release-b": progress(70)]
            )
        )
        let view = NSView()
        var shown: [BridgeDownloadTransferProgress?] = []
        let following = FrameFollowing<BridgeDownloadTransferProgress>(
            view: view
        ) { shown.append($0) }

        following.follow(store.downloadProgress(forRelease: "release-a"))
        following.follow(store.downloadProgress(forRelease: "release-c"))
        following.stop()

        #expect(shown == [progress(10), nil])
    }
}
