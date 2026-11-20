public struct TrackGroup: Sendable {
    /// The side or disc the group's tracks sit on, which heads the group.
    public var side: BridgeTrackSide
    public var tracks: [Track]
    public var totalDuration: BridgeDurationUnits?
    /// Whether the release has more than one group, so each shows its own play
    /// time beside the release's. Core decides.
    public var showsTime: Bool
    /// The place among `tracks` of the first one the second column lists, or
    /// `nil` when one column lists them all. Core decides.
    public var secondColumnStart: Int?

    /// This group's play time in the words core chose ("24 min"), or empty
    /// when no track reports a length.
    public var totalDurationText: String {
        totalDuration?.text ?? ""
    }

    public init(from bridge: BridgeTrackGroup) {
        side = bridge.side
        tracks = bridge.tracks.map(Track.init(from:))
        totalDuration = bridge.totalDuration
        showsTime = bridge.showsTime
        secondColumnStart = bridge.secondColumnStart.map(Int.init)
    }
}
