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
        let hosted = SnapshotTestSupport.hostInWindow(
            QueuePanel(onClose: {}, onInsertTracks: { _, _ in })
                .environment(Playback.stub())
                .environment(store)
                .environment(Queue.stub())
                .environment(ImageStore.stub())
                .environment(LibraryStore())
                .environment(ui),
            size: NSSize(width: 420, height: 720)
        )
        hosted.window.isReleasedWhenClosed = false
        defer { hosted.window.close() }
        await SnapshotTestSupport.settle(hosted.host)
        let localPoint = CGPoint(
            x: point.x,
            y: hosted.host.isFlipped
                ? point.y : hosted.host.bounds.height - point.y
        )
        let windowPoint = hosted.host.convert(localPoint, to: nil)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = try #require(
                NSEvent.mouseEvent(
                    with: type,
                    location: windowPoint,
                    modifierFlags: [],
                    timestamp: ProcessInfo.processInfo.systemUptime,
                    windowNumber: hosted.window.windowNumber,
                    context: nil,
                    eventNumber: 0,
                    clickCount: 1,
                    pressure: type == .leftMouseDown ? 1 : 0
                )
            )
            hosted.window.sendEvent(event)
        }
        await SnapshotTestSupport.settle(hosted.host)
        #expect(ui.activeSection == .library)
        #expect(ui.libraryBrowserMode == .albums)
        #expect(ui.selectedAlbumId == "playing-album")
        #expect(ui.pendingAlbumReveal?.albumId == "playing-album")
        #expect(ui.pendingTrackFlash?.trackId == "playing-track")
        #expect(ui.showQueue)
    }
}
