#if DEBUG
    import BaeKit

    /// Sample data the album grid's SwiftUI previews read. The rest of the
    /// fixtures live in `Fixtures/`, compiled only into the test and shot targets.
    enum PreviewData {
        /// The grid list, derived from the detail payloads so the grid summaries and
        /// the seeded `releaseDetails` agree on release ids. Ordered a-01..a-23,
        /// matching `albumDetailList`.
        static let albums: [BridgeAlbum] = albumDetailList.map(\.album)
    }
#endif
