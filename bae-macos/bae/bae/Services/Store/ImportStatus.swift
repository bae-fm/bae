import Foundation

enum ImportStatus: Equatable {
    case importing(progressPercent: UInt32, phase: String?, statusText: String?)
    case complete(albumId: String, releaseId: String)
    case error(message: String)
}
