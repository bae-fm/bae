import Foundation

enum ImportStatus: Equatable {
    case importing(progressPercent: UInt32, step: BridgeImportStep?)
    case complete(albumId: String, releaseId: String)
    /// The import failed. Carries the typed error; the UI renders the generic
    /// category line and offers the opaque detail in a copyable disclosure.
    case error(BridgeError)
}
