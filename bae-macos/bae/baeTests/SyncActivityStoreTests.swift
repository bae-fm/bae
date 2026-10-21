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

    /// An album or release still arriving names its device; one that has
    /// arrived names none.
    @MainActor
    @Test("arrivals are looked up by album and by release")
    func arrivalLookups() {
        let store = SyncActivityStore(
            activity: .preview(needsAttention: false),
            retryNow: {},
            dismissAbandoned: { _ in }
        )
        #expect(store.arrivingFrom(albumId: "a-1") == "Studio Mac")
        #expect(store.arrivingFrom(albumId: "a-9") == nil)
        #expect(store.arrival(releaseId: "r-1")?.trackIds == ["t-3", "t-4"])
        store.apply(settled())
        #expect(store.arrivingFrom(albumId: "a-1") == nil)
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
            downloads: nil,
            arrivals: .ready(devices: [], albums: [:], releases: [:])
        )
    }
}
