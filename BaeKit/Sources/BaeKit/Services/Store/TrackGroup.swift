public struct TrackGroup: Sendable {
    public var side: BridgeTrackSide
    /// The catalog key for the header word ("core.track.side" / "core.track.disc"),
    /// or `nil` for a flat single-disc group. Core-rendered onto the bridge row.
    public var headerKey: String?
    public var tracks: [Track]
    public var totalDuration: BridgeDurationUnits?
    /// Whether the release has more than one group, so each shows its own play
    /// time beside the release's. Core decides.
    public var showsTime: Bool

    /// This group's play time in the words core chose ("24 min"), or empty
    /// when no track reports a length.
    public var totalDurationText: String {
        totalDuration?.text ?? ""
    }

    /// The group header for the current locale: "Side A" / "Disc 2", or empty
    /// for a flat single-disc group (no header). bae-library decides the case and
    /// the side letter / disc number, and hands over the header word's catalog
    /// key; the UI resolves the word and substitutes the letter / number.
    public var sideHeaderText: String {
        side.headerText(key: headerKey)
    }

    public init(from bridge: BridgeTrackGroup) {
        side = bridge.side
        headerKey = bridge.headerKey
        tracks = bridge.tracks.map(Track.init(from:))
        totalDuration = bridge.totalDuration
        showsTime = bridge.showsTime
    }
}
