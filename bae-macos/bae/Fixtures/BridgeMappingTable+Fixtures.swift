import BaeKit
import Foundation

@testable import bae

extension BridgeMappingTable {
    /// A table of `trackSides`, holding as many track rows as they draw, as
    /// core counts them.
    init(
        images: [BridgeMappingImage],
        trackSides: [BridgeTrackTableSide],
        files: [BridgeMappingFileRow],
        untitledTrackCount: UInt32
    ) {
        self.init(
            images: images,
            trackSides: trackSides,
            files: files,
            trackCount: UInt32(trackSides.flatMap(\.rows).count),
            untitledTrackCount: untitledTrackCount
        )
    }

    /// Every track row in table order.
    var trackRows: [BridgeTrackTableRow] {
        trackSides.flatMap(\.rows)
    }
}

extension BridgeTrackTableSide {
    /// The rows on this side, in table order.
    var rows: [BridgeTrackTableRow] {
        runs.flatMap(\.rows)
    }
}
