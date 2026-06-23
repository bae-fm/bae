import Foundation

enum RepeatMode: Equatable {
    case off
    case track
    case context

    init(bridge: BridgeRepeatMode) {
        switch bridge {
        case .off: self = .off
        case .track: self = .track
        case .context: self = .context
        }
    }
}
