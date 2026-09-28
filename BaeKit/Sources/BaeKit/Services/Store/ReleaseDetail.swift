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
    /// What a list of the album's releases calls this one, in the current
    /// locale.
    public var displayName: String
    /// The pressing on one line: year, where and what it is, and its play
    /// time. Empty when the release states none of it.
    public var pressingLine: String
    /// The labels on one line: each name once, each catalog number once, as
    /// core groups them. Empty when the release names no label.
    public var labelsLine: String
    /// What the pressing is, worded: "Japan · 2×CD" and "Promo · Reissue".
    /// Either is empty when the release states none of its parts.
    public var pressingSummary: String
    public var pressingDetails: String
    public var totalDuration: BridgeDurationUnits?
    public var tracks: [Track]
    public var trackGroups: [TrackGroup]
    public var files: [BridgeFile]
    public var imageFiles: [BridgeFile]
    public var coverFiles: [BridgeFile]
    public var galleryItems: [BridgeGalleryItem]
    /// Every catalog that describes this release, in the order core lists
    /// them. Empty when no catalog does.
    public var records: [BridgeReleaseRecord]

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
        displayName = bridge.name.text
        pressingSummary = PressingText.line(bridge.pressingSummary)
        pressingDetails = PressingText.line(bridge.pressingDetails)
        let separator = QueueSummary.message("core.audio.list_separator")
        // The play time ends the line, in the words core chose for it
        // ("39 min" / "1 hr, 18 min"); absent when no track reports a length.
        pressingLine = [
            bridge.year.map { String($0) }, pressingSummary, pressingDetails,
            bridge.totalDuration?.text,
        ]
        .compactMap { $0 }
        .filter { !$0.isEmpty }
        .joined(separator: separator)
        labelsLine = Self.labelsLine(bridge.labels, separator: separator)
        totalDuration = bridge.totalDuration
        tracks = bridge.tracks.map(Track.init(from:))
        trackGroups = bridge.trackGroups.map(TrackGroup.init(from:))
        files = bridge.files
        imageFiles = bridge.imageFiles
        coverFiles = bridge.coverFiles
        galleryItems = bridge.galleryItems
        records = bridge.records
    }

    /// Each line core grouped: its names, then the catalog numbers they
    /// share, the lines one after another.
    static func labelsLine(
        _ lines: [BridgeLabelLine],
        separator: String
    ) -> String {
        let within = QueueSummary.message("core.label.list_separator")
        return
            lines.map { line in
                [
                    line.names.joined(separator: within),
                    line.catalogNumbers.joined(separator: within),
                ]
                .filter { !$0.isEmpty }
                .joined(separator: separator)
            }
            .filter { !$0.isEmpty }
            .joined(separator: separator)
    }
}
