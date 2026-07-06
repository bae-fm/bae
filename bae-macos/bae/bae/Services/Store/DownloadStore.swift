import SwiftUI

/// Mirror of core's in-memory download (pin) queue snapshot, rendered by the
/// Storage Manager's Downloads pane. UI event and projection paths land the
/// whole `BridgeDownloadSnapshot`; views read it at the leaf. The snapshot is
/// swapped wholesale (no per-item interning) because core exposes it in full.
@Observable
class DownloadStore {
    var snapshot: BridgeDownloadSnapshot

    init(snapshot: BridgeDownloadSnapshot) {
        self.snapshot = snapshot
    }

    func applySnapshot(_ snapshot: BridgeDownloadSnapshot) {
        self.snapshot = snapshot
    }
}
