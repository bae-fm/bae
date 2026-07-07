import MediaPlayer
import Testing

@testable import bae

@Suite("MediaControlService")
struct MediaControlServiceTests {
    @MainActor
    @Test("preview idle clears Now Playing")
    func previewIdleClearsNowPlaying() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer {
            infoCenter.nowPlayingInfo = nil
        }
        let service = MediaControlService()

        service.updateNowPlayingForPreview(
            state: .playing(
                path: "/tmp/Preview Track.flac",
                durationMs: 120_000
            )
        )
        #expect(infoCenter.nowPlayingInfo != nil)

        service.updateNowPlayingForPreview(state: .idle)

        #expect(infoCenter.nowPlayingInfo == nil)
    }
}
