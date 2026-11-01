#if os(macOS)
    /// One apply of a catalog release as the app follows it: the bridge's
    /// apply, which the pick or the release editor's apply is handed, and
    /// the network read it waits on, for the view that draws it. Make one for
    /// each apply the app shows; the steps stop being read when it is
    /// released. Its stream is pulled on `host`'s runtime.
    public final class CatalogApplySteps: Sendable {
        public let apply: BridgeCatalogApply
        public let steps: LatestSource<BridgeCatalogApplyStep>
        private let feed: LatestFeed

        public init(host: BridgeHost) {
            apply = BridgeCatalogApply(host: host)
            let stream = LatestStream<BridgeCatalogApplyStep>
                .catalogApplySteps()
            feed = stream.feed(from: apply)
            steps = stream.only
        }
    }
#endif
