import Foundation

/// A reference to a host-provided library image (a cover or an artist image):
/// the image id plus a content version. The UI fetches the bytes by id and
/// caches the decoded image under `(id, version)`, so a grid of covers renders
/// without re-crossing the bridge on scroll yet reloads when a cover is
/// replaced. `version` is the image row's content version, which moves when the
/// bytes change. Mirrors `BridgeImageRef`.
struct ImageRef: Equatable, Hashable, Sendable {
    let id: String
    let version: String

    init(bridge: BridgeImageRef) {
        id = bridge.id
        version = bridge.version
    }
}
