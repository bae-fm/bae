import Foundation

#if os(macOS)
    /// Import-flow preview audio — the in-place playback of candidate
    /// audio files in the import inspector. Distinct from `Playback`,
    /// which drives the library's now-playing track. The backing bridge
    /// methods are desktop-only (there is no import flow on iOS), so the
    /// whole type is gated to macOS.
    public final class PreviewAudio: Sendable, Observable {
        public let previewPlay: @Sendable (_ path: String) -> Void
        public let previewStop: @Sendable () -> Void
        public let previewTogglePause: @Sendable () -> Void
        public let previewSeekByRatio: @Sendable (_ ratio: Double) -> Void

        public init(
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

        public convenience init(handle: any AppHandleProtocol) {
            self.init(
                previewPlay: { handle.previewPlay(path: $0) },
                previewStop: { handle.previewStop() },
                previewTogglePause: { handle.previewTogglePause() },
                previewSeekByRatio: { handle.previewSeekByRatio(ratio: $0) }
            )
        }

        #if DEBUG
            // periphery:ignore
            public static let stub = PreviewAudio()
        #endif
    }
#endif
