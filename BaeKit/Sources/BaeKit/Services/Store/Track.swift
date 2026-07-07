import Foundation

public struct Track: Identifiable {
    public let id: String
    public var title: String
    public var durationMs: Int64?
    public var artistNames: String
    public var positionText: String

    public var durationLabel: String { DurationClock.text(durationMs) }

    public init(from bridge: BridgeTrack) {
        id = bridge.id
        title = bridge.title
        durationMs = bridge.durationMs
        artistNames = bridge.artistNames
        positionText = bridge.positionText
    }
}
