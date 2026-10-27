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
    private let release = BridgeUploadTransferKey.release(
        releaseId: "release-a"
    )

    private func transfer(_ bytesDone: UInt64) -> BridgeUploadTransfer {
        BridgeUploadTransfer(
            bar: BridgeUploadBar(bytesDone: bytesDone, bytesTotal: 100),
            throughputBps: 10
        )
    }

    @Test("a release's rate follows each change that names it")
    func releaseRateFollowsItsChanges() {
        let store = OutboxStore(uploadsPaused: false)
        store.applyTransferChange(
            .reset(
                rates: [
                    BridgeKeyedUploadRate(key: release, throughputBps: 10)
                ],
                total: transfer(10)
            )
        )
        #expect(store.throughput(forRelease: "release-a") == 10)

        store.applyTransferChange(.updated(key: release, throughputBps: 40))
        #expect(store.throughput(forRelease: "release-a") == 40)

        store.applyTransferChange(.removed(key: release))
        #expect(store.throughput(forRelease: "release-a") == nil)
    }

    @Test("the queue's rate shows only while bytes move")
    func queueRateShowsOnlyWhileBytesMove() {
        let store = OutboxStore(uploadsPaused: false)
        #expect(store.throughputText == nil)

        store.applyTransferChange(.totalUpdated(transfer: transfer(10)))
        #expect(
            store.throughputText
                == QueueSummary.throughputText(bytesPerSecond: 10)
        )

        store.applyTransferChange(
            .totalUpdated(
                transfer: BridgeUploadTransfer(bar: nil, throughputBps: 0)
            )
        )
        #expect(store.throughputText == nil)
    }
}

@Suite("Storage upload presentation")
struct StorageUploadPresentationTests {
    private func upload(
        queued: UInt32,
        uploading: UInt32,
        bar: BridgeUploadBar?
    ) -> BridgeReleaseUpload {
        BridgeReleaseUpload(
            progress: BridgeUploadProgress(
                queued: queued,
                uploading: uploading,
                activity: uploading > 0 ? .uploading : .queued
            ),
            bar: bar
        )
    }

    @Test("a row being sent carries its release rate and progress")
    func sendingRowCarriesRateAndProgress() {
        let observation = StorageUploadObservation(
            upload: upload(
                queued: 0,
                uploading: 1,
                bar: BridgeUploadBar(bytesDone: 32, bytesTotal: 100)
            ),
            throughputBps: 3_200_000
        )

        #expect(observation.progressFraction == 0.32)
        #expect(
            observation.throughputText
                == QueueSummary.throughputText(bytesPerSecond: 3_200_000)
        )
    }

    /// A release's own line names its phase; how many of its files are in
    /// that phase belongs to the list of those files.
    @Test("an active upload line is its phase and encrypted bytes, no count")
    func activeUploadIncludesPhaseAndBytes() {
        let observation = StorageUploadObservation(
            upload: upload(
                queued: 3,
                uploading: 2,
                bar: BridgeUploadBar(bytesDone: 600, bytesTotal: 1016)
            ),
            throughputBps: 0
        )
        let done = Int64(600).formatted(.byteCount(style: .file))
        let total = Int64(1016).formatted(.byteCount(style: .file))

        #expect(observation.transitionPhaseText == "Uploading")
        #expect(observation.progressDetailText?.contains(done) == true)
        #expect(observation.progressDetailText?.contains(total) == true)
        #expect(observation.throughputText == nil)
    }

    @Test("a release's file list summary counts the files in its phase")
    func fileListSummaryCountsFiles() {
        let progress = BridgeUploadProgress(
            queued: 3,
            uploading: 2,
            activity: .uploading
        )

        #expect(progress.fileCountText == "2 uploading")
    }

    /// Queued files nothing is moving, as with no storage connected, would
    /// otherwise draw a bar that never ends.
    @Test("files that are only waiting are queued and draw no bar")
    func waitingFilesDrawNoBar() {
        let observation = StorageUploadObservation(
            upload: upload(queued: 2, uploading: 0, bar: nil),
            throughputBps: nil
        )

        #expect(observation.transitionPhaseText == "Queued")
        #expect(observation.transitionStatusText == "Queued")
        #expect(observation.progressFraction == nil)
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
