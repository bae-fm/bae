import Foundation

enum RepeatMode: Equatable {
    case none
    case track
    case album

    init(bridge: BridgeRepeatMode) {
        switch bridge {
        case .none: self = .none
        case .track: self = .track
        case .album: self = .album
        }
    }
}
