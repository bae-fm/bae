import BaeKit
import Testing

@testable import bae

private final class ImportSelectionHandle: AppHandle, @unchecked Sendable {
    private var candidateCallback: (any ImportCandidateCallback)?
    private(set) var identifyCalls: [String] = []
    /// Every candidate held open, in order, with the subscription holding it.
    private(set) var openings:
        [(key: String, subscription: RecordedSubscription)] =
            []

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

    override func openImportCandidate(
        candidateKey: String,
        callback _: any OpenImportCandidateCallback
    ) -> LiveSubscription {
        let subscription = RecordedSubscription()
        openings.append((candidateKey, subscription))
        return subscription
    }

    /// The candidates held open right now.
    var heldOpen: [String] {
        openings.filter { !$0.subscription.cancelled }.map(\.key)
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

private final class RecordedSubscription: LiveSubscription,
    @unchecked Sendable
{
    private(set) var cancelled = false

    init() {
        super.init(noHandle: LiveSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func cancel() {
        cancelled = true
    }
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

    /// The one candidate whose pane shows is held open in core, so its
    /// result reads as seen; several selected show no one pane and hold none,
    /// and moving the selection lets go of the one before.
    @Test("a lone selected candidate is held open")
    func aLoneSelectedCandidateIsHeldOpen() {
        let handle = ImportSelectionHandle()
        let observations = ImportSelectionObservations(
            appHandle: handle,
            importStore: ImportStore(),
            uiStore: UiStore()
        )

        observations.selectionChanged(["first"])
        #expect(handle.heldOpen == ["first"])

        observations.selectionChanged(["first", "second"])
        #expect(handle.heldOpen.isEmpty)

        observations.selectionChanged(["second"])
        #expect(handle.heldOpen == ["second"])

        observations.selectionChanged(["second"])
        #expect(
            handle.openings.count == 2,
            "a selection that stays is held once"
        )

        observations.selectionChanged([])
        #expect(handle.heldOpen.isEmpty)
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
