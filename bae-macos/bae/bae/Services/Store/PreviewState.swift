import Foundation

struct ActivePreview {
    let path: String
    let isPlaying: Bool
    let durationMs: UInt64
}

enum PreviewState: Equatable {
    case idle
    case playing(path: String, durationMs: UInt64)
    case paused(path: String, durationMs: UInt64)

    var active: ActivePreview? {
        switch self {
        case .idle:
            nil
        case .playing(let path, let durationMs):
            ActivePreview(path: path, isPlaying: true, durationMs: durationMs)
        case .paused(let path, let durationMs):
            ActivePreview(path: path, isPlaying: false, durationMs: durationMs)
        }
    }
}
