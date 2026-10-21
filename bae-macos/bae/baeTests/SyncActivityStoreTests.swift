import BaeKit
import Testing

@Suite("SyncActivityStore")
struct SyncActivityStoreTests {
    /// The indicator shows while bytes move or something needs the person,
    /// and is absent for a settled library.
    @MainActor
    @Test("the indicator shows only while something moves or needs the person")
    func indicatorVisibility() {
        let store = SyncActivityStore(retryNow: {}, dismissAbandoned: { _ in })
        #expect(!store.visible)
        store.apply(.preview(needsAttention: false))
        #expect(store.visible)
        store.apply(settled())
        #expect(!store.visible)
        store.fail(BridgeError.Diagnostic(category: .internal, detail: "read"))
        #expect(store.visible)
    }

    private func settled() -> BridgeSyncActivity {
        BridgeSyncActivity(
            moving: false,
            needsAttention: false,
            failure: nil,
            attention: .ready(records: []),
            abandoned: .ready(attachments: []),
            syncing: false,
            uploads: nil,
            downloads: nil
        )
    }
}
