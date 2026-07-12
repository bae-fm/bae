extension BridgeRepeatMode {
    /// The next mode in a repeat button's cycle: off → context → track → off.
    /// UI-owned: core only accepts absolute `setRepeatMode` values; the caller
    /// computes the target from the mode it renders.
    public var next: BridgeRepeatMode {
        switch self {
        case .off: .context
        case .context: .track
        case .track: .off
        }
    }
}
