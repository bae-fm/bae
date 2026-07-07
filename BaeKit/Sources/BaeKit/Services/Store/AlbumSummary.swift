import Foundation
import Observation

/// What the grid card needs. Always complete once present.
/// `@Observable` class for identity-stable, per-field tracking.
///
/// `releaseIds` carries every release for this album (ordered by
/// created_at) so the detail view can enumerate releases without
/// loading a fat payload.
@Observable
public final class AlbumSummary: Identifiable {
    public let id: String
    public var title: String
    public var year: Int32?
    public var isCompilation: Bool
    public var artistNames: String
    public var releaseIds: [String]
    public var primaryReleaseId: String
    /// Reference to the album's cover image (id + content version), or nil when
    /// no cover is cached. Carried here so a cover change moves an observed
    /// field and the grid card re-renders; `ImageView` fetches the bytes by id
    /// and caches the decoded image under the version.
    public var cover: BridgeImageRef?

    public init(from bridge: BridgeAlbum) {
        id = bridge.id
        title = bridge.title
        year = bridge.year
        isCompilation = bridge.isCompilation
        artistNames = bridge.artistNames
        releaseIds = bridge.releaseIds
        primaryReleaseId = bridge.primaryReleaseId
        cover = bridge.cover
    }

    /// Per-field conditional assignment. Only fields that changed
    /// trigger @Observable re-render.
    public func update(from bridge: BridgeAlbum) {
        if title != bridge.title {
            title = bridge.title
        }
        if year != bridge.year {
            year = bridge.year
        }
        if isCompilation != bridge.isCompilation {
            isCompilation = bridge.isCompilation
        }
        if artistNames != bridge.artistNames {
            artistNames = bridge.artistNames
        }
        if releaseIds != bridge.releaseIds {
            releaseIds = bridge.releaseIds
        }
        if primaryReleaseId != bridge.primaryReleaseId {
            primaryReleaseId = bridge.primaryReleaseId
        }
        if cover != bridge.cover {
            cover = bridge.cover
        }
    }
}
