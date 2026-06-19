import Foundation

enum PreviewState: Equatable {
    case idle
    case playing(path: String, durationMs: UInt64)
    case paused(path: String, durationMs: UInt64)

    var active: (path: String, isPlaying: Bool, durationMs: UInt64)? {
        switch self {
        case .idle:
            nil
        case .playing(let path, let durationMs):
            (path, true, durationMs)
        case .paused(let path, let durationMs):
            (path, false, durationMs)
        }
    }
}
