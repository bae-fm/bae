import Foundation

/// Fat per-release projection for the album detail view. Composes a
/// [`ReleaseSummary`] (the slim fields) with the per-release data only the
/// detail view needs — tracks, files, gallery — all built from the one
/// `BridgeRelease` a detail read returned.
public struct ReleaseDetail: Identifiable, Sendable {
    public let summary: ReleaseSummary
    /// What a list of the album's releases calls this one, in the current
    /// locale.
    public var displayName: String
    /// What is printed on the release: its year when it differs from the
    /// album's, each label with its catalog numbers, then its country, as in
    /// "Harbor Records · HR-1042 · US". Empty when the release states none of
    /// it.
    public var editionLine: String
    /// What the person has: the media and details, then the play time, as in
    /// "2×LP · Remastered · 52 min". Empty when the release states none of
    /// it.
    public var copyLine: String
    /// The audio of the release's files: one format, or the facts they
    /// disagree on; nil when nothing read the files.
    public var sourceAudio: BridgeSourceAudioSummary?
    public var totalDuration: BridgeDurationUnits?
    public var tracks: [Track]
    public var trackGroups: [TrackGroup]
    /// Who did what on the release and its tracks, by role and artist, as
    /// core groups them.
    public var credits: [BridgeCreditRow]
    /// Whether the release has any credit, so its page shows a Credits
    /// section.
    public var hasCredits: Bool
    /// Whether any track of the release performs a work.
    public var performsWorks: Bool
    public var imageFiles: [BridgeFile]
    public var galleryItems: [BridgeGalleryItem]
    /// Every catalog that describes this release, in the order core lists
    /// them. Empty when no catalog does.
    public var records: [BridgeReleaseRecord]
    /// Whether any catalog describes this release.
    public var hasRecords: Bool
    /// Why this device can't play the release, in the current locale, or nil
    /// when it can: the device its audio is on ("On Ana's Mac"), or its
    /// originals missing here.
    public var unplayableLine: String?
    /// Where the release stands in this device's download queue, and what
    /// holds the queue back; nil when core offers no download control.
    public var downloadStatus: BridgeReleaseDownloadStatus?
    /// Where the release stands in moving to This Mac, in the same queue; nil
    /// when it is not moving to This Mac.
    public var moveToThisDevice: BridgeMoveToThisDeviceStatus?

    public var id: String {
        summary.id
    }

    /// Storage transitions the user can take right now, pre-computed by core
    /// from the release's state and whether the library syncs. Carried on the
    /// wrapped `summary`; the "Manage storage…" sheet renders one button per action
    /// and never derives availability.
    public var storageActions: [BridgeReleaseStorageAction] {
        summary.storageActions
    }

    public init(bridge: BridgeRelease) {
        summary = ReleaseSummary(from: bridge)
        displayName = bridge.name.text
        // The play time ends the copy line, in the words core chose for it
        // ("39 min" / "1 hr, 18 min"); absent when no track reports a length.
        editionLine = FactList.joined([
            bridge.distinctYear.map { PressingText.year($0) },
            Self.labelsText(bridge.labels),
            bridge.facts.area?.editionText,
        ])
        copyLine = FactList.joined([
            PressingText.line(bridge.mediaTerms),
            PressingText.line(bridge.pressingDetails),
            bridge.totalDuration?.text,
        ])
        sourceAudio = bridge.sourceAudio
        totalDuration = bridge.totalDuration
        downloadStatus = bridge.downloadStatus
        moveToThisDevice = bridge.moveToThisDevice
        tracks = bridge.tracks.map(Track.init(from:))
        trackGroups = bridge.trackGroups.map(TrackGroup.init(from:))
        credits = bridge.credits
        hasCredits = bridge.hasCredits
        performsWorks = bridge.performsWorks
        imageFiles = bridge.imageFiles
        galleryItems = bridge.galleryItems
        records = bridge.records
        hasRecords = bridge.hasRecords
        unplayableLine =
            bridgeReleasePlayabilityLine(
                playability: bridge.playability
            )?
            .localizedLine
    }

    /// Each line core grouped: its names, then the catalog numbers they
    /// share, the lines one after another.
    static func labelsText(_ lines: [BridgeLabelLine]) -> String {
        let within = localizedCoreString("core.label.list_separator")
        return FactList.joined(
            lines.map { line in
                FactList.joined([
                    line.names.joined(separator: within),
                    line.catalogNumbers.joined(separator: within),
                ])
            }
        )
    }
}
