import BaeKit

extension BridgeMetadataResult: Identifiable {
    /// Row identity is the release id — stable across a re-search of the same
    /// pressing, so SwiftUI keeps the row rather than tearing it down.
    public var id: String {
        releaseId
    }
}
