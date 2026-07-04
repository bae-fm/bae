import Observation

/// Mirror of core's in-memory download queue snapshot. Core pushes the whole
/// snapshot on every queue change; views render the bridge payload directly.
@Observable
final class DownloadStore {
    var snapshot: BridgeDownloadSnapshot

    init(snapshot: BridgeDownloadSnapshot) {
        self.snapshot = snapshot
    }
}
