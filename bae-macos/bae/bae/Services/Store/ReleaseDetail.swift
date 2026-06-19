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
    var totalDurationMs: Int64
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

    /// Total play time across all tracks, formatted for the current locale
    /// (e.g. "39 min" / "3 hr 42 min"). Empty when the release has no duration.
    var totalDurationText: String {
        guard totalDurationMs > 0 else { return "" }
        return Duration.milliseconds(totalDurationMs)
            .formatted(
                .units(allowed: [.hours, .minutes], width: .abbreviated)
            )
    }

    init(summary: ReleaseSummary, bridge: BridgeRelease) {
        self.summary = summary
        displayName = bridge.displayName
        compactMetadata = [
            bridge.year.map { String($0) },
            bridge.format,
            bridge.label,
            bridge.catalogNumber,
            bridge.country,
        ]
        .compactMap { $0 }.joined(separator: " \u{00B7} ")
        totalDurationMs = bridge.totalDurationMs
        tracks = bridge.tracks.map(Track.init(from:))
        trackGroups = bridge.trackGroups.map(TrackGroup.init(from:))
        files = bridge.files
        imageFiles = bridge.imageFiles
        galleryItems = bridge.galleryItems
    }
}
