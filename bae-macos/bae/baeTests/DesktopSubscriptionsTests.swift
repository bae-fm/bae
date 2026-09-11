import BaeKit
import Testing

@testable import bae

private final class ImportSelectionHandle: AppHandle, @unchecked Sendable {
    private var candidateCallback: (any ImportCandidateCallback)?
    private(set) var identifyCalls: [String] = []

    init() {
        super.init(noHandle: AppHandle.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func subscribeImportCandidate(
        candidateKey _: String,
        callback: any ImportCandidateCallback
    ) -> LiveSubscription {
        candidateCallback = callback
        return ImportSelectionSubscription()
    }

    override func rerunIdentifyForCandidate(candidateKey: String) {
        identifyCalls.append(candidateKey)
    }

    func deliver(_ detail: BridgeImportCandidateDetail) {
        candidateCallback?.onValue(value: detail)
    }

    /// The key names no scanned folder any more.
    func deliverNothing() {
        candidateCallback?.onValue(value: nil)
    }
}

private final class ImportSelectionSubscription: LiveSubscription,
    @unchecked Sendable
{
    init() {
        super.init(noHandle: LiveSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func cancel() {}
}

@MainActor
@Suite("Import candidate selection")
struct DesktopSubscriptionsTests {
    @Test("selecting an unseeded candidate never starts identification")
    func selectionDoesNotIdentify() async {
        let handle = ImportSelectionHandle()
        let store = ImportStore()
        let observations = ImportSelectionObservations(
            appHandle: handle,
            importStore: store,
            uiStore: UiStore()
        )

        observations.selectionChanged([MappingFixtures.candidateKey])
        handle.deliver(
            MappingFixtures.detail(
                mapping: nil,
                edit: MappingFixtures.blankEdit,
                metadataProvenance: nil
            )
        )
        for _ in 0..<100 where store.selectedCandidates.isEmpty {
            await Task.yield()
        }

        #expect(store.selectedCandidates.count == 1)
        #expect(handle.identifyCalls.isEmpty)
    }

    /// A pick is about a folder. When the read says there is no such folder
    /// any more, there is nothing left for the pick to claim.
    @Test("a folder that is gone cancels the pick made on it")
    func aGoneFolderCancelsThePick() async throws {
        let key = MappingFixtures.candidateKey
        let handle = ImportSelectionHandle()
        let store = ImportStore()
        let observations = ImportSelectionObservations(
            appHandle: handle,
            importStore: store,
            uiStore: UiStore()
        )

        observations.selectionChanged([key])
        handle.deliver(MappingFixtures.detail(mapping: nil))
        for _ in 0..<100 where store.selectedCandidates.isEmpty {
            await Task.yield()
        }
        _ = try #require(
            store.beginMetadataApplication(
                key: key,
                provenance: MappingFixtures.provenance
            )
        )

        handle.deliverNothing()
        for _ in 0..<100
        where store.metadataApplicationSession(forKey: key) != nil {
            await Task.yield()
        }

        #expect(store.metadataApplicationSession(forKey: key) == nil)
    }
}
