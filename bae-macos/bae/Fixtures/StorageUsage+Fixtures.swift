import BaeKit
import Foundation
import SwiftUI

@testable import bae

extension BridgeDeviceStorage {
    /// A library with pinned releases, a cache and the library's own
    /// records.
    static let previewUsual = make(
        [
            (.pinned, 6_100_000_000), (.cache, 1_800_000_000),
            (.library, 420_000_000),
        ]
    )

    /// The same, with data a reset left behind.
    static let previewBeforeReset = make(
        [
            (.pinned, 6_100_000_000), (.cache, 1_800_000_000),
            (.library, 420_000_000), (.beforeReset, 960_000_000),
        ]
    )

    /// A library with nothing pinned.
    static let previewNothingPinned = make(
        [(.cache, 220_000_000), (.library, 420_000_000)]
    )

    /// What core measured as `parts`, with the choices the cache offers.
    static func make(
        _ parts: [(BridgeStorageKind, UInt64)]
    ) -> BridgeDeviceStorage {
        let limit: (BridgeCacheLimitChoice, UInt64) -> BridgeCacheLimit = {
            BridgeCacheLimit(choice: $0, bytes: $1)
        }
        let cache = parts.first { $0.0 == .cache }?.1 ?? 0
        return BridgeDeviceStorage(
            total: parts.reduce(0) { $0 + $1.1 },
            parts: parts.map {
                BridgeStoragePart(kind: $0.0, bytes: $0.1)
            },
            cache: cache,
            canClearCache: cache > 0,
            cacheLimit: limit(.gb20, 20_000_000_000),
            cacheLimits: [
                limit(.gb1, 1_000_000_000), limit(.gb5, 5_000_000_000),
                limit(.gb10, 10_000_000_000), limit(.gb20, 20_000_000_000),
                limit(.gb50, 50_000_000_000),
            ],
            beforeReset: parts.first { $0.0 == .beforeReset }?.1
        )
    }
}
