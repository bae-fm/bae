import BaeKit
import Testing

@testable import bae

/// The usage part draws one segment for each kind core lists, in core's
/// order, each in its kind's own color.
@MainActor
@Suite("Storage usage")
struct StorageUsageTests {
    @Test("the segments follow core's parts, in order, with their shares")
    func segmentsFollowTheParts() {
        let usage = StorageUsage(.previewBeforeReset)

        #expect(
            usage.segments.map(\.kind)
                == [.pinned, .cache, .library, .beforeReset]
        )
        #expect(
            usage.segments.map(\.bytes) == [
                6_100_000_000, 1_800_000_000, 420_000_000, 960_000_000,
            ]
        )
        #expect(usage.total == 9_280_000_000)
        let shares = usage.segments.map(\.share)
        #expect(abs(shares.reduce(0, +) - 1) < 1e-9)
        #expect(abs(shares[0] - 6_100_000_000.0 / 9_280_000_000.0) < 1e-9)
    }

    @Test("a kind core leaves out gets no segment")
    func leftOutKindsGetNoSegment() {
        let usage = StorageUsage(.previewNothingPinned)

        #expect(usage.segments.map(\.kind) == [.cache, .library])
    }

    @Test("each kind keeps its own accent, whichever kinds show")
    func eachKindKeepsItsAccent() {
        #expect(BridgeStorageKind.pinned.accent == .blue)
        #expect(BridgeStorageKind.cache.accent == .gold)
        #expect(BridgeStorageKind.library.accent == .orchid)
        #expect(BridgeStorageKind.beforeReset.accent == .mint)
        #expect(
            StorageUsage(.previewNothingPinned).segments.map(\.kind.accent)
                == [.gold, .orchid]
        )
    }

    @Test("nothing measured gives segments of no share")
    func emptyTotalHasNoShare() {
        let usage = StorageUsage(.make([(.library, 0)]))

        #expect(usage.segments.map(\.share) == [0])
    }
}
