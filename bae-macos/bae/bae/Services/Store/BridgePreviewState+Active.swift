import BaeKit

struct ActivePreview {
    let target: BridgePreviewTarget
    let isPlaying: Bool
}

extension BridgePreviewState {
    var active: ActivePreview? {
        switch self {
        case .idle:
            nil
        case .playing(let target, _):
            ActivePreview(target: target, isPlaying: true)
        case .paused(let target, _):
            ActivePreview(target: target, isPlaying: false)
        }
    }
}
