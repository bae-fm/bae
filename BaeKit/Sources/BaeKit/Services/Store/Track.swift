import Foundation

public struct Track: Identifiable {
    public let id: String
    public var title: String
    public var durationClock: BridgeDurationClock?
    public var artistNames: String
    /// The artist to show on the row, or `nil` for none — core's decision (set
    /// only for a compilation). A work-release view shows `artistNames` instead;
    /// that is navigation context, decided at the view, not here.
    public var displayArtist: String?
    public var positionText: String

    public var durationLabel: String { DurationClock.label(durationClock) }

    public init(from bridge: BridgeTrack) {
        id = bridge.id
        title = bridge.title
        durationClock = bridge.durationClock
        artistNames = bridge.artistNames
        displayArtist = bridge.displayArtist
        positionText = bridge.positionText
    }
}
