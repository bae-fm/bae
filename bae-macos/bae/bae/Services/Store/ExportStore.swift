import SwiftUI

/// Mirror of core's in-memory export queue snapshot, rendered by the Storage
/// Manager's Exporting pane. The reducer is the sole writer: it lands the whole
/// `BridgeExportSnapshot` on every `exportQueueChanged` event; views read it at
/// the leaf. The snapshot is swapped wholesale (no per-item interning) because
/// core re-pushes it in full on every change.
@Observable
class ExportStore {
    var snapshot: BridgeExportSnapshot

    init(snapshot: BridgeExportSnapshot) {
        self.snapshot = snapshot
    }
}
