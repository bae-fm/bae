import Foundation

enum ImportStatus: Equatable {
    case importing(progressPercent: UInt32, step: BridgeImportStep?)
    case complete(albumId: String, releaseId: String)
    case error(message: String)
}
