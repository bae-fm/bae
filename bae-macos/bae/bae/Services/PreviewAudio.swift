import Foundation

/// Import-flow preview audio — the in-place playback of candidate
/// audio files in the import inspector. Distinct from `Playback`,
/// which drives the library's now-playing track.
final class PreviewAudio: Sendable, Observable {
    let previewPlay: @Sendable (_ path: String) -> Void
    let previewStop: @Sendable () -> Void
    let previewTogglePause: @Sendable () -> Void
    let previewSeekByRatio: @Sendable (_ ratio: Double) -> Void

    init(
        previewPlay: @escaping @Sendable (String) -> Void = { _ in },
        previewStop: @escaping @Sendable () -> Void = {},
        previewTogglePause: @escaping @Sendable () -> Void = {},
        previewSeekByRatio: @escaping @Sendable (Double) -> Void = { _ in }
    ) {
        self.previewPlay = previewPlay
        self.previewStop = previewStop
        self.previewTogglePause = previewTogglePause
        self.previewSeekByRatio = previewSeekByRatio
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            previewPlay: { handle.previewPlay(path: $0) },
            previewStop: { handle.previewStop() },
            previewTogglePause: { handle.previewTogglePause() },
            previewSeekByRatio: { handle.previewSeekByRatio(ratio: $0) }
        )
    }

    // periphery:ignore
    static let stub = PreviewAudio()
}
