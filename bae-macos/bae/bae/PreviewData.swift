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

    extension BridgeDurationClock {
        /// The clock a track `ms` milliseconds long shows ("3:07"), for the
        /// fixtures that build tracks by hand: whole seconds, with hours only
        /// from one hour up.
        static func length(ms: UInt64) -> BridgeDurationClock {
            let seconds = ms / 1000
            let hours = seconds / 3600
            return BridgeDurationClock(
                negative: false,
                hours: hours > 0 ? hours : nil,
                minutes: UInt32((seconds % 3600) / 60),
                seconds: UInt32(seconds % 60)
            )
        }
    }
#endif
