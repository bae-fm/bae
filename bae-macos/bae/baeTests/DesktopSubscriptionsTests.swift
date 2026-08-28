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

    override func identifyFolderForLookup(candidateKey: String) {
        identifyCalls.append(candidateKey)
    }

    func deliver(_ detail: BridgeImportCandidateDetail) {
        candidateCallback?.onValue(value: detail)
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
            uiStore: UiStore(),
            configStore: PreviewData.configStore()
        )

        observations.selectionChanged([MappingFixtures.candidateKey])
        handle.deliver(
            MappingFixtures.detail(mapping: nil, metadataSeed: nil)
        )
        for _ in 0..<100 where store.selectedCandidates.isEmpty {
            await Task.yield()
        }

        #expect(store.selectedCandidates.count == 1)
        #expect(handle.identifyCalls.isEmpty)
    }
}
