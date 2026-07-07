import BaeKit
import Foundation

enum ImportStatus: Equatable {
    case importing(progressPercent: UInt32, step: BridgeImportStep?)
    case complete(albumId: String, releaseId: String)
    /// The import failed. Carries the typed error; the UI renders the generic
    /// category line and offers the opaque detail in a copyable disclosure.
    case error(BridgeError)

    init(bridge: BridgeCandidateImportStatus) {
        switch bridge {
        case .importing(let progressPercent, let step):
            self = .importing(progressPercent: progressPercent, step: step)
        case .complete(let releaseId, let albumId):
            self = .complete(albumId: albumId, releaseId: releaseId)
        case .error(let error):
            self = .error(error)
        }
    }
}
