/// How playback is set: its volume, whether it is muted, and its repeat mode.
/// Core sends the three together in every playback value.
public struct PlaybackSettings: Equatable, Sendable {
    public let volume: Float
    public let isMuted: Bool
    public let repeatMode: BridgeRepeatMode

    public init(volume: Float, isMuted: Bool, repeatMode: BridgeRepeatMode) {
        self.volume = volume
        self.isMuted = isMuted
        self.repeatMode = repeatMode
    }

    init(_ values: BridgePlaybackValues) {
        self.init(
            volume: values.volume,
            isMuted: values.isMuted,
            repeatMode: values.repeatMode
        )
    }
}
