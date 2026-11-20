import AppKit
import BaeKit
import Combine
import SwiftUI
import Testing

@testable import bae

@Suite("Queue now-playing navigation", .serialized)
struct QueueNowPlayingNavigationTests {
    /// The reveal request waiting for a grid to take it: no grid listens in
    /// these tests.
    @MainActor
    private func waitingReveal(in ui: UiStore) async throws -> AlbumReveal? {
        var reveals: [AlbumReveal] = []
        let listening = ui.albumReveals.requests.sink {
            reveals.append($0)
            Wait.happened()
        }
        defer { listening.cancel() }
        try await Wait.until { !reveals.isEmpty }
        return reveals.first
    }

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
        ui.setSidePanePresented(true)
        let store = PlaybackStore()
        store.apply(
            .playing(
                track: BridgeNowPlayingTrack(
                    track: BridgePlayingTrack(
                        trackId: "playing-track",
                        durationMs: 180_000
                    ),
                    display: .library(
                        display: BridgeTrackDisplay(
                            title: "Track Title",
                            artistLine: .named("Track Artist"),
                            albumId: "playing-album",
                            releaseId: "playing-release",
                            albumTitle: "Album Title",
                            coverImage: nil
                        )
                    )
                )
            )
        )
        try await SnapshotTestSupport.withHostedWindow(
            MacQueuePanelContainer(onClose: {}, onInsertTracks: { _, _ in })
                .environment(Playback.stub())
                .environment(store)
                .environment(Queue.stub())
                .environment(ImageStore.stub())
                .environment(LibraryProjectionStore(library: .stub()))
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
            #expect(
                try await waitingReveal(in: ui)
                    == AlbumReveal(
                        albumId: "playing-album",
                        trackId: "playing-track"
                    )
            )
            // No release details are loaded: the release comes with the
            // playing track, not from the library's cache.
            #expect(
                ui.selectedReleaseIdByAlbum["playing-album"]
                    == "playing-release"
            )
            #expect(ui.showSidePane)
        }
    }

    /// Core's value that removes the playing track can arrive after the view
    /// drew Go to Now Playing enabled and before the person chooses it.
    @MainActor
    @Test(
        "going to a track the library no longer holds says something went wrong"
    )
    func goingToARemovedTrackSaysSomethingWentWrong() {
        let ui = UiStore()
        ui.navigateToImport()
        let store = PlaybackStore()
        store.apply(
            .playing(
                track: BridgeNowPlayingTrack(
                    track: BridgePlayingTrack(
                        trackId: "playing-track",
                        durationMs: 180_000
                    ),
                    display: .removed
                )
            )
        )

        NowPlayingNavigationAction(playbackStore: store, uiStore: ui).perform()

        #expect(ui.activeSection != .library)
        #expect(
            ui.lastError?.line
                == localizedCoreString("core.error.category.internal")
        )
    }
}
