import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Queue now-playing navigation", .serialized)
struct QueueNowPlayingNavigationTests {
    @MainActor
    @Test(
        "the card reveals the playing track from its art, text, progress, and padding",
        arguments: [
            CGPoint(x: 50, y: 95), CGPoint(x: 150, y: 91),
            CGPoint(x: 180, y: 132), CGPoint(x: 19, y: 70),
        ]
    )
    func cardRevealsTrack(point: CGPoint) async throws {
        let ui = UiStore()
        ui.navigateToImport()
        ui.setLibraryBrowserMode(.artists)
        ui.setQueuePresented(true)
        let store = PlaybackStore()
        store.play(
            track: NowPlayingTrack(
                trackId: "playing-track",
                trackTitle: "Track Title",
                artistNames: "Track Artist",
                albumId: "playing-album",
                coverImage: nil,
                durationMs: 180_000
            )
        )
        try await SnapshotTestSupport.withHostedWindow(
            QueuePanel(onClose: {}, onInsertTracks: { _, _ in })
                .environment(Playback.stub())
                .environment(store)
                .environment(Queue.stub())
                .environment(ImageStore.stub())
                .environment(LibraryStore())
                .environment(ui),
            size: NSSize(width: 420, height: 720)
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let localPoint = CGPoint(
                x: point.x,
                y: host.isFlipped
                    ? point.y : host.bounds.height - point.y
            )
            let windowPoint = host.convert(localPoint, to: nil)
            try HostedInput.click(at: windowPoint, in: window)
            try await SnapshotTestSupport.settle(host)
            #expect(ui.activeSection == .library)
            #expect(ui.libraryBrowserMode == .albums)
            #expect(ui.selectedAlbumId == "playing-album")
            #expect(ui.pendingAlbumReveal?.albumId == "playing-album")
            #expect(ui.pendingTrackFlash?.trackId == "playing-track")
            #expect(ui.showQueue)
        }
    }
}
