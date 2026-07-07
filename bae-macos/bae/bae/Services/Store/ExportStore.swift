import BaeKit
import SwiftUI

/// Mirror of core's in-memory export queue snapshot, rendered by the Storage
/// Manager's Exporting pane. UI event and projection paths land the whole
/// `BridgeExportSnapshot`; views read it at the leaf. The snapshot is swapped
/// wholesale (no per-item interning) because core exposes it in full.
@Observable
class ExportStore {
    var snapshot: BridgeExportSnapshot

    init(snapshot: BridgeExportSnapshot) {
        self.snapshot = snapshot
    }

    func applySnapshot(_ snapshot: BridgeExportSnapshot) {
        self.snapshot = snapshot
    }
}
