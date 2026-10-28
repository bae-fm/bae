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
    private let release = "release-a"

    private func transfer(_ bytesDone: UInt64) -> BridgeUploadTransfer {
        BridgeUploadTransfer(
            bar: BridgeUploadBar(bytesDone: bytesDone, bytesTotal: 100),
            throughputBps: 10
        )
    }

    private func releaseTransfer(
        _ bytesDone: UInt64,
        _ throughputBps: UInt64
    ) -> BridgeReleaseUploadTransfer {
        BridgeReleaseUploadTransfer(
            bar: BridgeUploadBar(bytesDone: bytesDone, bytesTotal: 100),
            throughputBps: throughputBps
        )
    }

    @Test("a release's bytes and rate follow each change that names it")
    func releaseTransferFollowsItsChanges() {
        let store = OutboxStore(uploadsPaused: false)
        store.applyTransferChange(
            .reset(
                releases: [
                    BridgeKeyedReleaseUploadTransfer(
                        releaseId: release,
                        transfer: releaseTransfer(20, 10)
                    )
                ],
                total: transfer(10)
            )
        )
        let source = store.releaseTransfer(forRelease: release)
        var heard: [BridgeReleaseUploadTransfer?] = []
        let observation = source.values.observe(source.key) {
            heard.append($0)
        }

        store.applyTransferChange(
            .updated(releaseId: release, transfer: releaseTransfer(60, 40))
        )
        store.applyTransferChange(.removed(releaseId: release))
        source.values.stopObserving(observation)

        #expect(
            heard == [releaseTransfer(20, 10), releaseTransfer(60, 40), nil]
        )
    }

    /// The file's share as its native view reads it on a frame.
    private func share(_ store: OutboxStore, _ fileId: String) -> Double? {
        store.fileShare(forFile: fileId).latest
    }

    @Test("a file's share follows each change that names it")
    func fileShareFollowsItsChanges() {
        let store = OutboxStore(uploadsPaused: false)
        store.applyFileTransferChange(
            .reset(files: [BridgeKeyedFileTransfer(fileId: "a", fraction: 0.25)]
            )
        )
        #expect(share(store, "a") == 0.25)
        #expect(share(store, "b") == nil)

        store.applyFileTransferChange(.updated(fileId: "a", fraction: 0.5))
        store.applyFileTransferChange(.updated(fileId: "b", fraction: 0.1))
        #expect(share(store, "a") == 0.5)
        #expect(share(store, "b") == 0.1)

        store.applyFileTransferChange(.removed(fileId: "a"))
        #expect(share(store, "a") == nil)
    }

    @Test("the files being sent together follow each total delivered")
    func totalFollowsEachDelivery() {
        let store = OutboxStore(uploadsPaused: false)
        var heard: [BridgeUploadTransfer?] = []
        let observation = store.totalTransferStream.observe {
            heard.append($0)
        }

        store.applyTransferChange(.totalUpdated(transfer: transfer(10)))
        let idle = BridgeUploadTransfer(bar: nil, throughputBps: 0)
        store.applyTransferChange(.totalUpdated(transfer: idle))
        store.totalTransferStream.stopObserving(observation)

        #expect(heard == [nil, transfer(10), idle])
    }
}

@Suite("ReleaseEditor cloud batches")
@MainActor
struct ReleaseEditorCloudBatchTests {
    @Test("the selected releases cross the bridge in one command")
    func selectionCrossesTheBridgeTogether() async throws {
        let recorder = CloudBatchRecorder()
        let editor = ReleaseEditor(
            moveReleasesToCloud: { releaseIds in
                await recorder.append(releaseIds: releaseIds)
                return .complete(
                    receipt: cloudUploadReceipt(releaseIds)
                )
            }
        )

        try await editor.moveReleasesToCloud(["release-a", "release-b"])

        let calls = await recorder.calls
        #expect(calls == [["release-a", "release-b"]])
    }

    @Test(
        "a partial admission surfaces the refusal"
    )
    func partialAdmissionSurfacesItsRefusal() async {
        let refusal = BridgeError.Diagnostic(
            category: .internal,
            detail: "release release-b already has an active storage transition"
        )
        let editor = ReleaseEditor(
            moveReleasesToCloud: { _ in
                .partial(
                    receipt: cloudUploadReceipt(["release-a"]),
                    failure: BridgeUploadBatchFailure(
                        releaseIds: ["release-b"],
                        error: refusal
                    )
                )
            }
        )

        do {
            try await editor.moveReleasesToCloud(["release-a", "release-b"])
            Issue.record("the refused release must surface its error")
        }
        catch let error as BridgeError {
            guard case .Diagnostic(let category, let detail) = error else {
                Issue.record("the typed storage refusal must cross unchanged")
                return
            }
            #expect(category == .internal)
            #expect(detail.contains("release-b"))
        }
        catch {
            Issue.record("the refusal must remain a BridgeError")
        }
    }
}

private actor CloudBatchRecorder {
    /// The releases of each call, in order.
    private(set) var calls: [[String]] = []

    func append(releaseIds: [String]) {
        calls.append(releaseIds)
    }
}
