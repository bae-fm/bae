import Foundation

enum PreviewState: Equatable {
    case idle
    case playing(path: String, durationMs: UInt64, durationLabel: String)
    case paused(path: String, durationMs: UInt64, durationLabel: String)

    var active:
        (
            path: String, isPlaying: Bool, durationMs: UInt64,
            durationLabel: String
        )?
    {
        switch self {
        case .idle:
            nil
        case .playing(let path, let durationMs, let durationLabel):
            (path, true, durationMs, durationLabel)
        case .paused(let path, let durationMs, let durationLabel):
            (path, false, durationMs, durationLabel)
        }
    }
}
