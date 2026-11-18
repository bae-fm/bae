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
        /// The clock core shows for a track `ms` milliseconds long ("3:07"),
        /// for the fixtures that build tracks by hand.
        static func length(ms: UInt64) -> BridgeDurationClock {
            guard let clock = bridgeClock(ms: Int64(ms)) else {
                preconditionFailure(
                    "core labels every length that is not negative"
                )
            }
            return clock
        }
    }
#endif
