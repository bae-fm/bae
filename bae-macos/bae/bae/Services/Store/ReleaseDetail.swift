import Foundation

/// Fat per-release projection for the album detail view. Composes a
/// [`ReleaseSummary`] (slim fields identity-stable in the `releases`
/// slice) with the per-release data only the detail view needs —
/// tracks, files, gallery. Replaced wholesale on update, so consumers
/// read fields through the struct value rather than subscribing to
/// identity.
///
/// The wrapped `summary` lives in the `releases` slice; interning a
/// detail interns its summary too, so every consumer of the detail
/// also sees the same identity-stable summary instance.
struct ReleaseDetail: Identifiable {
    let summary: ReleaseSummary
    var displayName: String
    var compactMetadata: String
    var totalDurationLabel: String
    var tracks: [Track]
    var trackGroups: [TrackGroup]
    var files: [BridgeFile]
    var imageFiles: [BridgeFile]
    var galleryItems: [BridgeGalleryItem]

    var id: String {
        summary.id
    }

    /// Storage transitions the user can take right now, pre-computed by core
    /// from the release's state and cloud-home presence. Carried on the
    /// wrapped `summary`; the "Storage…" sheet renders one button per action
    /// and never derives availability.
    var storageActions: [BridgeReleaseStorageAction] {
        summary.storageActions
    }

    init(summary: ReleaseSummary, bridge: BridgeRelease) {
        self.summary = summary
        displayName = bridge.displayName
        compactMetadata = bridge.compactMetadata
        totalDurationLabel = bridge.totalDurationLabel
        tracks = bridge.tracks.map(Track.init(from:))
        trackGroups = bridge.trackGroups.map(TrackGroup.init(from:))
        files = bridge.files
        imageFiles = bridge.imageFiles
        galleryItems = bridge.galleryItems
    }
}
