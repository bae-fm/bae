import BaeKit
import Testing

@testable import bae

/// The Bridge* records are generated at build from `bae-bridge/src/types.rs`;
/// these start from `OutboxStore.emptySnapshot` and set only the fields the
/// derived `hasPendingCloudWork` reads.
@Suite("OutboxStore.hasPendingCloudWork")
struct OutboxStoreHasPendingCloudWorkTests {

    private func store(
        _ mutate: (inout BridgeOutboxSnapshot) -> Void
    ) -> OutboxStore {
        var snapshot = OutboxStore.emptySnapshot
        mutate(&snapshot)
        return OutboxStore(snapshot: snapshot)
    }

    @Test("an idle queue has no pending cloud work")
    func idleQueueHasNoPendingWork() {
        let store = OutboxStore(snapshot: OutboxStore.emptySnapshot)
        #expect(store.hasPendingCloudWork == false)
    }

    @Test("a queued upload group counts as pending cloud work")
    func queuedUploadIsPending() {
        let store = store { snapshot in
            snapshot.uploadGroups = [
                BridgeUploadReleaseGroup(
                    releaseId: "release-a",
                    displayTitle: "Release A",
                    files: [
                        BridgeUploadFileOp(
                            fileId: "file-1",
                            label: .filename(name: "01 Track Title.flac"),
                            bar: nil,
                            sourceBytesTotal: 1000,
                            state: .queued,
                            lastError: nil
                        )
                    ],
                    progress: OutboxStore.emptySnapshot.total
                )
            ]
        }
        #expect(store.hasPendingCloudWork)
    }

    @Test("a pending delete with no uploads counts as pending cloud work")
    func pendingDeleteIsPending() {
        let store = store { snapshot in
            snapshot.pendingDeletes = 1
        }
        #expect(store.hasPendingCloudWork)
    }

    @Test("an older delivery cannot replace newer queue state")
    func staleSnapshotCannotRegressTheStore() {
        var current = OutboxStore.emptySnapshot
        current.revision = 2
        let store = OutboxStore(snapshot: current)

        var stale = OutboxStore.emptySnapshot
        stale.revision = 1
        stale.pendingDeletes = 1
        store.applySnapshot(stale)

        #expect(store.snapshot.revision == 2)
        #expect(store.snapshot.pendingDeletes == 0)
    }
}

@Suite("StorageStatusBand cloud-transition actions")
@MainActor
struct StorageStatusBandCloudTransitionTests {
    @Test("only an unwindable durable transition can be cancelled")
    func cancellationAvailabilityFollowsTheProjectedPhase() {
        var active = OutboxStore.emptySnapshot.total
        active.queued = 1
        active.activity = .queued
        active.canCancel = true
        #expect(StorageUploadObservation.active(active).canCancel)

        var cancelling = active
        cancelling.queued = 0
        cancelling.cancelling = 1
        cancelling.activity = .cancelling
        cancelling.canCancel = false
        #expect(StorageUploadObservation.active(cancelling).canCancel == false)

        var publishing = active
        publishing.queued = 0
        publishing.publishing = 1
        publishing.activity = .publishing
        publishing.canCancel = false
        #expect(StorageUploadObservation.active(publishing).canCancel == false)

        #expect(StorageUploadObservation.queueing.canCancel == false)
        #expect(StorageUploadObservation.awaiting.canCancel == false)
    }

    @Test("publication still suppresses storage actions")
    func publicationSuppressesStorageActions() {
        var progress = OutboxStore.emptySnapshot.total
        progress.publishing = 1
        progress.activity = .publishing

        #expect(
            StorageStatusBand.showsTransferActions(
                uploadObservation: .active(progress)
            )
                == false
        )
    }

    @Test("a resting release shows its storage actions")
    func restingReleaseShowsStorageActions() {
        #expect(
            StorageStatusBand.showsTransferActions(uploadObservation: nil)
        )
    }
}

@Suite("Cloud upload pause presentation")
@MainActor
struct CloudUploadPausePresentationTests {
    @Test("a pause failure reaches the storage manager error alert")
    func pauseFailureIsDisplayed() async {
        let uiStore = UiStore()
        let sync = Sync(
            setSyncPaused: { _ in throw StubError.notImplemented }
        )

        await OutboxSection.setPaused(true, sync: sync, uiStore: uiStore)

        #expect(uiStore.lastError != nil)
    }

    @Test("the queue reports the absolute paused state")
    func stoppedQueueReportsPaused() {
        #expect(
            OutboxSection.pauseStatusText(.paused)
                == String(localized: "Paused")
        )
        #expect(OutboxSection.pauseStatusText(.running) == nil)
    }

    @Test("pause state does not replace the durable queue counts")
    func pausedQueueKeepsItsCounts() {
        let status = QueueSectionHeaderStatus(
            pauseStatusText: String(localized: "Paused"),
            summaryText: QueueSummary.countLabel("core.queue.queued", 14)
        )

        #expect(status.pauseText == String(localized: "Paused"))
        #expect(
            status.summaryText
                == QueueSummary.countLabel("core.queue.queued", 14)
        )
    }
}

@Suite("Cloud import queue presentation")
struct CloudImportQueuePresentationTests {
    @Test("a restored import rejoins its release's durable upload")
    func restoredImportRejoinsDurableUpload() {
        var snapshot = OutboxStore.emptySnapshot
        snapshot.perRelease["release-a"] = snapshot.total
        let store = OutboxStore(snapshot: snapshot)

        guard
            case .active = store.persistedUploadObservation(
                forRelease: "release-a"
            )
        else {
            Issue.record("the restored import reads the durable release upload")
            return
        }
    }

    @Test("an active upload line includes its exact phase and provider bytes")
    func activeUploadIncludesPhaseAndBytes() {
        var progress = OutboxStore.emptySnapshot.total
        progress.uploading = 1
        progress.bar = BridgeUploadBar(
            phase: .uploading,
            bytesDone: 600,
            bytesTotal: 1016
        )
        progress.activity = .uploading

        let line = UploadObservation.active(progress).statusText
        let done = Int64(600).formatted(.byteCount(style: .file))
        let total = Int64(1016).formatted(.byteCount(style: .file))

        #expect(
            line.contains(QueueSummary.countLabel("core.queue.uploading", 1))
        )
        #expect(line.contains(done))
        #expect(line.contains(total))
    }

    @Test("the byte label counts the phase the bar fills with, not the badge")
    func byteLabelFollowsTheBarsOwnPhase() {
        var progress = OutboxStore.emptySnapshot.total
        progress.queued = 1
        progress.uploading = 1
        // A file is uploading, but another is still unprepared, so core counts
        // the queue's source bytes; the badge still leads with the upload.
        let bar = BridgeUploadBar(
            phase: .preparing,
            bytesDone: 1_000,
            bytesTotal: 1_100
        )
        progress.bar = bar
        progress.activity = .uploading

        let line = UploadObservation.active(progress).statusText

        #expect(
            line.contains(QueueSummary.countLabel("core.queue.uploading", 1))
        )
        #expect(line.contains(bar.text))
        #expect(
            bar.text
                != BridgeUploadBar(
                    phase: .uploading,
                    bytesDone: 1_000,
                    bytesTotal: 1_100
                )
                .text
        )
    }

    @Test("a failed attempt is presented as retrying")
    func failedAttemptIsRetrying() {
        var progress = OutboxStore.emptySnapshot.total
        progress.retrying = 1
        progress.activity = .retrying

        #expect(
            progress.activityText
                == QueueSummary.countLabel("core.outbox.retrying", 1)
        )
    }

    @Test("waiting for the first retained queue value is still queued")
    func awaitingQueueIsQueued() {
        #expect(
            UploadObservation.awaiting.statusText
                == QueueSummary.countLabel("core.queue.queued", 1)
        )
    }

    @Test("the import row keeps a progress bar through the cloud transition")
    func cloudTransitionProgressBar() {
        var progress = OutboxStore.emptySnapshot.total
        progress.bar = BridgeUploadBar(
            phase: .uploading,
            bytesDone: 500,
            bytesTotal: 2_000
        )

        #expect(UploadObservation.awaiting.progressBar == .indeterminate)
        #expect(
            UploadObservation.active(progress).progressBar
                == .determinate(0.25)
        )
    }
}

@Suite("Storage cloud-upload handoff")
@MainActor
struct StorageCloudUploadHandoffTests {
    @Test(
        "the row remains in a cloud transition until its retained queue value arrives"
    )
    func commandHandsOffWithoutAStandingLocalFrame() {
        let store = OutboxStore(snapshot: OutboxStore.emptySnapshot)

        store.beginCloudUpload(forRelease: "release-a")
        #expect(
            store.storageUploadObservation(forRelease: "release-a")
                == .queueing
        )

        store.cloudUploadQueued(forRelease: "release-a", atRevision: 2)
        #expect(
            store.storageUploadObservation(forRelease: "release-a")
                == .awaiting
        )

        var active = OutboxStore.emptySnapshot
        active.revision = 2
        active.perRelease["release-a"] = active.total
        store.applySnapshot(active)
        guard
            case .active = store.storageUploadObservation(
                forRelease: "release-a"
            )
        else {
            Issue.record("the retained queue owns the release after handoff")
            return
        }
    }

    @Test("a queue value that wins the callback race remains authoritative")
    func queueValueMayArriveBeforeTheCommandReceipt() {
        let store = OutboxStore(snapshot: OutboxStore.emptySnapshot)
        store.beginCloudUpload(forRelease: "release-a")

        var active = OutboxStore.emptySnapshot
        active.revision = 2
        active.perRelease["release-a"] = active.total
        store.applySnapshot(active)
        store.cloudUploadQueued(forRelease: "release-a", atRevision: 2)

        guard
            case .active = store.storageUploadObservation(
                forRelease: "release-a"
            )
        else {
            Issue.record("an older command receipt cannot replace queue state")
            return
        }
    }

    @Test("a refused enqueue returns the release to its resting state")
    func failedCommandEndsItsLocalTransition() {
        let store = OutboxStore(snapshot: OutboxStore.emptySnapshot)
        store.beginCloudUpload(forRelease: "release-a")
        store.cloudUploadFailed(forRelease: "release-a")

        #expect(
            store.storageUploadObservation(forRelease: "release-a") == nil
        )
    }
}
