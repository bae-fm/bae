import SwiftUI

/// Mirror of core's in-memory download (pin) queue snapshot, rendered by the
/// Storage Manager's Downloads pane. The reducer is the sole writer: it lands
/// the whole `BridgeDownloadSnapshot` on every `downloadQueueChanged` event;
/// views read it at the leaf. The snapshot is swapped wholesale (no per-item
/// interning) because core re-pushes it in full on every change.
@Observable
class DownloadStore {
    var snapshot: BridgeDownloadSnapshot

    init(snapshot: BridgeDownloadSnapshot) {
        self.snapshot = snapshot
    }
}
