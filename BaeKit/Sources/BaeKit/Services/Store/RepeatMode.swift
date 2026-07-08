import Foundation

public enum RepeatMode: Equatable {
    case off
    case track
    case context

    public init(bridge: BridgeRepeatMode) {
        switch bridge {
        case .off: self = .off
        case .track: self = .track
        case .context: self = .context
        }
    }

    public var bridge: BridgeRepeatMode {
        switch self {
        case .off: .off
        case .track: .track
        case .context: .context
        }
    }
}
