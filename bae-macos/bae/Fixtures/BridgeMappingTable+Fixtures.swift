import BaeKit
import Foundation

@testable import bae

extension BridgeMappingTable {
    /// A table of `trackSides`, holding as many track rows as they draw, as
    /// core counts them, and each list as core hands it: `nil` when empty.
    init(
        images: [BridgeMappingImage],
        trackSides: [BridgeTrackTableSide],
        files: [BridgeMappingFileRow],
        untitledTrackCount: UInt32
    ) {
        self.init(
            images: images.isEmpty ? nil : images,
            trackSides: trackSides.isEmpty ? nil : trackSides,
            files: files.isEmpty ? nil : files,
            trackCount: UInt32(trackSides.flatMap(\.rows).count),
            untitledTrackCount: untitledTrackCount
        )
    }

    /// Every track row in table order.
    var trackRows: [BridgeTrackTableRow] {
        (trackSides ?? []).flatMap(\.rows)
    }
}

extension BridgeTrackTableSide {
    /// The rows on this side, in table order.
    var rows: [BridgeTrackTableRow] {
        runs.flatMap(\.rows)
    }
}
