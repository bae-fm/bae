import Foundation
import Observation

/// What the grid card needs. Always complete once present.
/// `@Observable` class for identity-stable, per-field tracking.
///
/// `releaseIds` carries every release for this album (ordered by
/// created_at) so the detail view can enumerate releases without
/// loading a fat payload.
@Observable
final class AlbumSummary: Identifiable {
    let id: String
    var title: String
    var year: Int32?
    var isCompilation: Bool
    var artistNames: String
    var releaseIds: [String]
    var primaryReleaseId: String
    /// Cache-bustable identifier for the cover (`<path>#v=<mtime>`), or nil when
    /// no cover is cached. Carried here so a cover change moves an observed
    /// field and the grid card re-renders; `ImageView` strips the `#v=…` suffix
    /// before reading the file.
    var coverPath: String?

    init(from bridge: BridgeAlbum) {
        id = bridge.id
        title = bridge.title
        year = bridge.year
        isCompilation = bridge.isCompilation
        artistNames = bridge.artistNames
        releaseIds = bridge.releaseIds
        primaryReleaseId = bridge.primaryReleaseId
        coverPath = bridge.coverPath
    }

    /// Per-field conditional assignment. Only fields that changed
    /// trigger @Observable re-render.
    func update(from bridge: BridgeAlbum) {
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
        if coverPath != bridge.coverPath {
            coverPath = bridge.coverPath
        }
    }
}
