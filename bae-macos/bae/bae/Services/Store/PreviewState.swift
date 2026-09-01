import BaeKit
import Foundation

struct ActivePreview {
    let target: BridgePreviewTarget
    let isPlaying: Bool
    let durationMs: UInt64
}

enum PreviewState: Equatable {
    case idle
    case playing(target: BridgePreviewTarget, durationMs: UInt64)
    case paused(target: BridgePreviewTarget, durationMs: UInt64)

    var active: ActivePreview? {
        switch self {
        case .idle:
            nil
        case .playing(let target, let durationMs):
            ActivePreview(
                target: target,
                isPlaying: true,
                durationMs: durationMs
            )
        case .paused(let target, let durationMs):
            ActivePreview(
                target: target,
                isPlaying: false,
                durationMs: durationMs
            )
        }
    }
}
