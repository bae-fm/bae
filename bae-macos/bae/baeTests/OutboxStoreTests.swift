import BaeKit
import Testing

@testable import bae

private func cloudUploadReceipt(
    _ releaseIds: [String],
    revision: UInt64 = 2
) -> BridgeMakeRemoteReceipt {
    BridgeMakeRemoteReceipt(
        outboxRevision: revision,
        releaseIds: releaseIds
    )
}

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
                    progress: OutboxStore.emptySnapshot.total,
                    throughputBps: 0
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
        #expect(
            StorageUploadObservation.active(
                progress: active,
                throughputBps: 0
            )
            .canCancel
        )

        var cancelling = active
        cancelling.queued = 0
        cancelling.cancelling = 1
        cancelling.activity = .cancelling
        cancelling.canCancel = false
        #expect(
            StorageUploadObservation.active(
                progress: cancelling,
                throughputBps: 0
            )
            .canCancel == false
        )

        var publishing = active
        publishing.queued = 0
        publishing.publishing = 1
        publishing.activity = .publishing
        publishing.canCancel = false
        #expect(
            StorageUploadObservation.active(
                progress: publishing,
                throughputBps: 0
            )
            .canCancel == false
        )

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
                uploadObservation: .active(
                    progress: progress,
                    throughputBps: 0
                )
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

@Suite("Cloud import queue presentation")
struct CloudImportQueuePresentationTests {
    @Test("an active storage row carries its release rate and progress")
    func activeStorageRowCarriesRateAndProgress() {
        var progress = OutboxStore.emptySnapshot.total
        progress.uploading = 1
        progress.bar = BridgeUploadBar(
            phase: .uploading,
            bytesDone: 32,
            bytesTotal: 100
        )
        progress.activity = .uploading
        let observation = StorageUploadObservation.active(
            progress: progress,
            throughputBps: 3_200_000
        )

        #expect(observation.progressBar.fraction == 0.32)
        #expect(
            observation.throughputText
                == QueueSummary.throughputText(bytesPerSecond: 3_200_000)
        )
    }

    @Test("a restored import rejoins its release's durable upload")
    func restoredImportRejoinsDurableUpload() {
        var snapshot = OutboxStore.emptySnapshot
        snapshot.perRelease["release-a"] = BridgeReleaseUploadProgress(
            progress: snapshot.total,
            throughputBps: 0
        )
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

    @Test("a missing source is primary while the retry count remains available")
    func missingSourceIsActionable() {
        var progress = OutboxStore.emptySnapshot.total
        progress.retrying = 1
        progress.activity = .retrying
        progress.issue = .sourceUnavailable(paths: [
            "/Volumes/Library/Album/01 Track.flac"
        ])

        #expect(
            progress.primaryActivityText
                == QueueSummary.message("core.outbox.source_unavailable")
        )
        #expect(
            progress.activityText
                == QueueSummary.countLabel("core.outbox.retrying", 1)
        )
        #expect(
            progress.sourceUnavailablePaths
                == ["/Volumes/Library/Album/01 Track.flac"]
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

        let command = store.beginCloudUploads(forReleases: ["release-a"])
        #expect(
            store.storageUploadObservation(forRelease: "release-a")
                == .queueing
        )

        store.finishCloudUploads(
            for: command,
            receipt: cloudUploadReceipt(["release-a"])
        )
        #expect(
            store.storageUploadObservation(forRelease: "release-a")
                == .awaiting
        )

        var active = OutboxStore.emptySnapshot
        active.revision = 2
        active.perRelease["release-a"] = BridgeReleaseUploadProgress(
            progress: active.total,
            throughputBps: 0
        )
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
        let command = store.beginCloudUploads(forReleases: ["release-a"])

        var active = OutboxStore.emptySnapshot
        active.revision = 2
        active.perRelease["release-a"] = BridgeReleaseUploadProgress(
            progress: active.total,
            throughputBps: 0
        )
        store.applySnapshot(active)
        store.finishCloudUploads(
            for: command,
            receipt: cloudUploadReceipt(["release-a"])
        )

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
        let command = store.beginCloudUploads(forReleases: ["release-a"])
        store.finishCloudUploads(for: command, receipt: nil)

        #expect(
            store.storageUploadObservation(forRelease: "release-a") == nil
        )
    }

    @Test("an active target remains active while its sibling hands off")
    func activeTargetDoesNotBlockItsSiblingHandoff() {
        var snapshot = OutboxStore.emptySnapshot
        snapshot.perRelease["release-b"] = BridgeReleaseUploadProgress(
            progress: snapshot.total,
            throughputBps: 0
        )
        let store = OutboxStore(snapshot: snapshot)

        let command = store.beginCloudUploads(
            forReleases: ["release-a", "release-b"]
        )
        store.finishCloudUploads(
            for: command,
            receipt: cloudUploadReceipt(["release-a"])
        )
        #expect(
            store.storageUploadObservation(forRelease: "release-a")
                == .awaiting
        )
        #expect(
            store.storageUploadObservation(forRelease: "release-b")
                == .active(progress: snapshot.total, throughputBps: 0)
        )
    }

    @Test("a batch handoff covers every release until the queue arrives")
    func batchHandoffCoversEveryRelease() {
        let store = OutboxStore(snapshot: OutboxStore.emptySnapshot)
        let releaseIds = ["release-a", "release-b"]

        let command = store.beginCloudUploads(forReleases: releaseIds)
        for releaseId in releaseIds {
            #expect(
                store.storageUploadObservation(forRelease: releaseId)
                    == .queueing
            )
        }

        store.finishCloudUploads(
            for: command,
            receipt: cloudUploadReceipt(releaseIds)
        )
        for releaseId in releaseIds {
            #expect(
                store.storageUploadObservation(forRelease: releaseId)
                    == .awaiting
            )
        }
    }

    @Test(
        "a partial receipt hands off admitted releases and rests refused ones"
    )
    func partialReceiptOwnsOnlyAdmittedReleases() {
        let store = OutboxStore(snapshot: OutboxStore.emptySnapshot)
        let command = store.beginCloudUploads(
            forReleases: ["release-a", "release-b"]
        )

        store.finishCloudUploads(
            for: command,
            receipt: cloudUploadReceipt(["release-a"])
        )

        #expect(
            store.storageUploadObservation(forRelease: "release-a")
                == .awaiting
        )
        #expect(
            store.storageUploadObservation(forRelease: "release-b") == nil
        )
    }

    @Test("an overlapping loser cannot clear the winning command's handoff")
    func overlappingCommandsKeepTheWinningHandoff() {
        let store = OutboxStore(snapshot: OutboxStore.emptySnapshot)
        let releaseIds = ["release-a"]

        let first = store.beginCloudUploads(forReleases: releaseIds)
        let second = store.beginCloudUploads(forReleases: releaseIds)
        store.finishCloudUploads(for: first, receipt: nil)

        #expect(
            store.storageUploadObservation(forRelease: "release-a")
                == .queueing
        )
        store.finishCloudUploads(
            for: second,
            receipt: cloudUploadReceipt(releaseIds)
        )
        #expect(
            store.storageUploadObservation(forRelease: "release-a")
                == .awaiting
        )
    }

    @Test("a late loser cannot clear an already-published handoff")
    func lateOverlappingFailureKeepsTheWinningHandoff() {
        let store = OutboxStore(snapshot: OutboxStore.emptySnapshot)
        let first = store.beginCloudUploads(forReleases: ["release-a"])
        let second = store.beginCloudUploads(forReleases: ["release-a"])

        store.finishCloudUploads(
            for: second,
            receipt: cloudUploadReceipt(["release-a"])
        )
        store.finishCloudUploads(for: first, receipt: nil)

        #expect(
            store.storageUploadObservation(forRelease: "release-a")
                == .awaiting
        )
    }

    @Test("a partial command leaves a refused release owned by its overlap")
    func partialOverlapKeepsEachWinningCommand() {
        let store = OutboxStore(snapshot: OutboxStore.emptySnapshot)
        let batch = store.beginCloudUploads(
            forReleases: ["release-a", "release-b"]
        )
        let overlapping = store.beginCloudUploads(forReleases: ["release-b"])

        store.finishCloudUploads(
            for: batch,
            receipt: cloudUploadReceipt(["release-a"], revision: 2)
        )
        #expect(
            store.storageUploadObservation(forRelease: "release-a")
                == .awaiting
        )
        #expect(
            store.storageUploadObservation(forRelease: "release-b")
                == .queueing
        )

        store.finishCloudUploads(
            for: overlapping,
            receipt: cloudUploadReceipt(["release-b"], revision: 3)
        )
        #expect(
            store.storageUploadObservation(forRelease: "release-b")
                == .awaiting
        )
    }
}

@Suite("ReleaseEditor cloud batches")
@MainActor
struct ReleaseEditorCloudBatchTests {
    @Test("the selected releases cross the bridge in one command")
    func selectionCrossesTheBridgeTogether() async throws {
        let recorder = CloudBatchRecorder()
        let store = OutboxStore(snapshot: OutboxStore.emptySnapshot)
        let editor = ReleaseEditor(
            moveReleasesToCloud: { releaseIds, pin in
                await recorder.append(releaseIds: releaseIds, pin: pin)
                return .complete(
                    receipt: cloudUploadReceipt(releaseIds)
                )
            },
            outboxStore: store
        )

        try await editor.moveReleasesToCloud(
            ["release-a", "release-b"],
            true
        )

        let calls = await recorder.calls
        #expect(calls.count == 1)
        #expect(calls.first?.releaseIds == ["release-a", "release-b"])
        #expect(calls.first?.pin == true)
    }

    @Test(
        "partial admission keeps the durable release and surfaces the refusal"
    )
    func partialAdmissionPreservesItsReceipt() async {
        let store = OutboxStore(snapshot: OutboxStore.emptySnapshot)
        let refusal = BridgeError.Diagnostic(
            category: .internal,
            detail: "release release-b already has an active storage transition"
        )
        let editor = ReleaseEditor(
            moveReleasesToCloud: { _, _ in
                .partial(
                    receipt: cloudUploadReceipt(["release-a"]),
                    failure: BridgeMakeRemoteBatchFailure(
                        releaseIds: ["release-b"],
                        error: refusal
                    )
                )
            },
            outboxStore: store
        )

        do {
            try await editor.moveReleasesToCloud(
                ["release-a", "release-b"],
                false
            )
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

        #expect(
            store.storageUploadObservation(forRelease: "release-a")
                == .awaiting
        )
        #expect(
            store.storageUploadObservation(forRelease: "release-b") == nil
        )
    }
}

private actor CloudBatchRecorder {
    struct Call: Sendable {
        let releaseIds: [String]
        let pin: Bool
    }

    private(set) var calls: [Call] = []

    func append(releaseIds: [String], pin: Bool) {
        calls.append(Call(releaseIds: releaseIds, pin: pin))
    }
}
