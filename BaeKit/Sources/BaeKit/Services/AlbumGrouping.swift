import Observation

/// Album view preferences shared by the Apple apps. Sort criteria still
/// describe the order inside each artist's group.
@MainActor
@Observable
public final class AlbumGrouping {
    public private(set) var byArtist = false

    public init() {}

    public func setByArtist(_ enabled: Bool) {
        byArtist = enabled
    }

}
