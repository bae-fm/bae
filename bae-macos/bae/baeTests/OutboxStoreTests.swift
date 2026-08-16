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
                            bytesDone: 0,
                            bytesTotal: 1000,
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
}
