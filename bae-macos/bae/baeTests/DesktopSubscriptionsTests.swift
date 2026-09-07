import BaeKit
import Testing

@testable import bae

private final class ImportSelectionHandle: AppHandle, @unchecked Sendable {
    private(set) var candidateCallbacks: [any ImportCandidateCallback] = []
    private(set) var selectionCallbacks: [any ImportSelectionCallback] = []
    private(set) var editorSubscriptions: [ImportSelectionSubscription] = []
    private(set) var selectionSubscriptions: [ImportSelectionSubscription] = []
    private(set) var identifyCalls: [String] = []

    init() { super.init(noHandle: AppHandle.NoHandle()) }
    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func subscribeImportCandidate(
        candidateKey: String,
        callback: any ImportCandidateCallback
    ) -> LiveSubscription {
        candidateCallbacks.append(callback)
        let subscription = ImportSelectionSubscription()
        editorSubscriptions.append(subscription)
        return subscription
    }

    override func subscribeImportSelection(
        candidateKeys: [String],
        callback: any ImportSelectionCallback
    ) -> LiveSubscription {
        selectionCallbacks.append(callback)
        let subscription = ImportSelectionSubscription()
        selectionSubscriptions.append(subscription)
        return subscription
    }

    override func subscribeOutputs(callback: any OutputCallback)
        -> LiveSubscription
    {
        ImportSelectionSubscription()
    }

    override func subscribeCandidateRuntime(
        callback: any CandidateRuntimeCallback
    ) -> LiveSubscription {
        ImportSelectionSubscription()
    }

    override func identifyFolderForLookup(candidateKey: String) {
        identifyCalls.append(candidateKey)
    }
}

private final class ImportSelectionSubscription: LiveSubscription,
    @unchecked Sendable
{
    private(set) var cancelled = false
    init() { super.init(noHandle: LiveSubscription.NoHandle()) }
    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }
    override func cancel() { cancelled = true }
}

@MainActor
@Suite("Import candidate selection")
struct DesktopSubscriptionsTests {
    @Test("bulk selection never opens album editors")
    func bulkSelectionDoesNotSubscribeToEditors() {
        let handle = ImportSelectionHandle()
        let observations = ImportSelectionObservations(
            appHandle: handle,
            importStore: ImportStore(),
            uiStore: UiStore()
        )
        observations.setEditorVisible(true)
        observations.selectionChanged(["/imports/first", "/imports/second"])
        #expect(handle.editorSubscriptions.isEmpty)
        #expect(handle.selectionSubscriptions.count == 1)
    }

    @Test("selecting an unseeded candidate never starts identification")
    func selectionDoesNotIdentify() async throws {
        let handle = ImportSelectionHandle()
        let store = ImportStore()
        let observations = ImportSelectionObservations(
            appHandle: handle,
            importStore: store,
            uiStore: UiStore()
        )
        observations.selectionChanged([MappingFixtures.candidateKey])
        #expect(handle.editorSubscriptions.isEmpty)
        observations.setEditorVisible(true)
        try #require(handle.candidateCallbacks.first)
            .onValue(
                value: MappingFixtures.detail(
                    mapping: nil,
                    edit: MappingFixtures.blankEdit,
                    metadataProvenance: nil
                )
            )
        await settle { store.editorCandidate != nil }
        #expect(store.editorCandidate?.key == MappingFixtures.candidateKey)
        #expect(handle.identifyCalls.isEmpty)
    }

    @Test("leaving the pane and choosing a bulk selection close the editor")
    func editorLifetimeFollowsVisibility() async throws {
        let handle = ImportSelectionHandle()
        let store = ImportStore()
        let observations = ImportSelectionObservations(
            appHandle: handle,
            importStore: store,
            uiStore: UiStore()
        )
        observations.selectionChanged([MappingFixtures.candidateKey])
        observations.setEditorVisible(true)
        try #require(handle.candidateCallbacks.first)
            .onValue(value: MappingFixtures.detail(mapping: nil))
        await settle { store.editorCandidate != nil }
        observations.setEditorVisible(false)
        #expect(handle.editorSubscriptions[0].cancelled)
        #expect(store.editorCandidate == nil)
        observations.setEditorVisible(true)
        #expect(handle.editorSubscriptions.count == 2)
        observations.selectionChanged([
            MappingFixtures.candidateKey, "/imports/second",
        ])
        #expect(handle.editorSubscriptions[1].cancelled)
        #expect(store.editorCandidate == nil)
        #expect(handle.selectionSubscriptions[0].cancelled)
    }

    @Test(
        "callbacks from a closed same-key editor cannot replace its new value"
    )
    func staleEditorDeliveryIsIgnored() async throws {
        let handle = ImportSelectionHandle()
        let store = ImportStore()
        let observations = ImportSelectionObservations(
            appHandle: handle,
            importStore: store,
            uiStore: UiStore()
        )
        observations.selectionChanged([MappingFixtures.candidateKey])
        observations.setEditorVisible(true)
        observations.setEditorVisible(false)
        observations.setEditorVisible(true)
        handle.candidateCallbacks[1]
            .onValue(
                value: MappingFixtures.detail(
                    mapping: nil,
                    folderName: "Current"
                )
            )
        await settle { store.editorCandidate?.displayName == "Current" }
        handle.candidateCallbacks[0]
            .onValue(
                value: MappingFixtures.detail(mapping: nil, folderName: "Old")
            )
        for _ in 0..<100 { await Task.yield() }
        #expect(store.editorCandidate?.displayName == "Current")
    }

    @Test(
        "deleted candidate keys leave the selection without loading their editors"
    )
    func missingKeysAreDeselected() async {
        let handle = ImportSelectionHandle()
        let store = ImportStore()
        let uiStore = UiStore()
        let observations = ImportSelectionObservations(
            appHandle: handle,
            importStore: store,
            uiStore: uiStore
        )
        uiStore.onFolderCandidateSelectionChanged = { [weak observations] in
            observations?.selectionChanged($0)
        }
        uiStore.setFolderCandidateSelection([
            "/imports/first", "/imports/second",
        ])
        handle.selectionCallbacks[0]
            .onValue(
                value: BridgeImportSelection(
                    candidateKeys: ["/imports/second"],
                    offers: [],
                    canCombine: false
                )
            )
        await settle { uiStore.selectedFolderCandidates == ["/imports/second"] }
        #expect(handle.selectionSubscriptions[0].cancelled)
        #expect(handle.selectionSubscriptions.count == 2)
        #expect(handle.editorSubscriptions.isEmpty)
        // An older response cannot remove the surviving key.
        handle.selectionCallbacks[0]
            .onValue(
                value: BridgeImportSelection(
                    candidateKeys: [],
                    offers: [],
                    canCombine: false
                )
            )
        handle.selectionCallbacks[1]
            .onValue(
                value: BridgeImportSelection(
                    candidateKeys: ["/imports/second"],
                    offers: [],
                    canCombine: false
                )
            )
        await settle { store.selection?.candidateKeys == ["/imports/second"] }
        #expect(uiStore.selectedFolderCandidates == ["/imports/second"])
    }

    @Test("dropping the subscription owner closes both core queries")
    func ownerClosesQueries() {
        let handle = ImportSelectionHandle()
        var observations: ImportSelectionObservations? =
            ImportSelectionObservations(
                appHandle: handle,
                importStore: ImportStore(),
                uiStore: UiStore()
            )
        observations?.setEditorVisible(true)
        observations?.selectionChanged([MappingFixtures.candidateKey])
        observations = nil
        #expect(handle.editorSubscriptions[0].cancelled)
        #expect(handle.selectionSubscriptions[0].cancelled)
    }

    @Test("dropping desktop wiring closes queries installed on the UI store")
    func desktopOwnerClosesInstalledQueries() {
        let handle = ImportSelectionHandle()
        let store = ImportStore()
        let uiStore = UiStore()
        uiStore.setFolderCandidateSelection(["/imports/first"])
        store.editorVisibility.send(true)
        var subscriptions: DesktopSubscriptions? = DesktopSubscriptions(
            appHandle: handle,
            importStore: store,
            outputStore: OutputStore(
                snapshot: BridgeOutputSnapshot(
                    outputs: [],
                    total: BridgeOutputProgress(
                        queued: 0,
                        active: 0,
                        failed: 0
                    ),
                    summaryParts: [],
                    paused: false
                )
            ),
            uiStore: uiStore
        )
        subscriptions?.start()
        #expect(handle.editorSubscriptions.count == 1)
        #expect(handle.selectionSubscriptions.count == 1)
        subscriptions = nil
        #expect(handle.editorSubscriptions[0].cancelled)
        #expect(handle.selectionSubscriptions[0].cancelled)
        uiStore.setFolderCandidateSelection(["/imports/second"])
        #expect(handle.editorSubscriptions.count == 1)
        #expect(handle.selectionSubscriptions.count == 1)
    }

    private func settle(_ arrived: () -> Bool) async {
        for _ in 0..<100 where !arrived() { await Task.yield() }
        #expect(arrived())
    }
}
