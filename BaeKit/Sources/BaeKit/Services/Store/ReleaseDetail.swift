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
public struct ReleaseDetail: Identifiable {
    public let summary: ReleaseSummary
    public var displayName: String
    public var compactMetadata: String
    public var totalDuration: BridgeDurationUnits?
    public var tracks: [Track]
    public var trackGroups: [TrackGroup]
    public var files: [BridgeFile]
    public var imageFiles: [BridgeFile]
    public var galleryItems: [BridgeGalleryItem]

    public var id: String {
        summary.id
    }

    /// Storage transitions the user can take right now, pre-computed by core
    /// from the release's state and cloud-home presence. Carried on the
    /// wrapped `summary`; the "Storage…" sheet renders one button per action
    /// and never derives availability.
    public var storageActions: [BridgeReleaseStorageAction] {
        summary.storageActions
    }

    public init(summary: ReleaseSummary, bridge: BridgeRelease) {
        self.summary = summary
        displayName = bridge.displayName
        // The play time ends the line, in the words core chose for it
        // ("39 min" / "1 hr, 18 min"); absent when no track reports a length.
        compactMetadata = [
            bridge.year.map { String($0) },
            bridge.format,
            bridge.label,
            bridge.catalogNumber,
            bridge.country,
            bridge.totalDuration?.text,
        ]
        .compactMap { $0 }.joined(separator: " \u{00B7} ")
        totalDuration = bridge.totalDuration
        tracks = bridge.tracks.map(Track.init(from:))
        trackGroups = bridge.trackGroups.map(TrackGroup.init(from:))
        files = bridge.files
        imageFiles = bridge.imageFiles
        galleryItems = bridge.galleryItems
    }
}
