import BaeKit
import SwiftUI

/// What Settings → Storage's usage part draws, from what core measured: the
/// total and one segment for each kind that holds bytes, in the order core
/// lists them. A kind core leaves out is not drawn.
struct StorageUsage: Equatable {
    struct Segment: Equatable, Identifiable {
        let kind: BridgeStorageKind
        let bytes: UInt64
        /// This kind's part of the total, from 0 to 1.
        let share: Double

        var id: BridgeStorageKind { kind }
    }

    let total: UInt64
    let segments: [Segment]

    init(_ storage: BridgeDeviceStorage) {
        total = storage.total
        segments = storage.parts.map { part in
            Segment(
                kind: part.kind,
                bytes: part.bytes,
                share: storage.total == 0
                    ? 0 : Double(part.bytes) / Double(storage.total)
            )
        }
    }
}

extension BridgeStorageKind {
    /// The accent whose selection step colors this kind, the same in both
    /// themes: blue, lime, orchid and mint keep apart for colour-blind
    /// viewers and each reaches 3:1 on the surface.
    var accent: AccentChoice {
        switch self {
        case .pinned: .blue
        case .cache: .lime
        case .library: .orchid
        case .beforeReset: .mint
        }
    }

    var color: Color { accent.buttonColor }

    var title: String {
        switch self {
        case .pinned: String(localized: "Pinned")
        case .cache: String(localized: "Cache")
        case .library: String(localized: "Library")
        case .beforeReset: String(localized: "From before the reset")
        }
    }
}
