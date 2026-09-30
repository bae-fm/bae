import AppKit
import BaeKit
import Observation
import SwiftUI
import Testing

@testable import bae

/// The grid revealing an album, and a track in the album's detail, as Go to
/// Now Playing and a track picked in search ask it to.
@MainActor
@Suite("AlbumGridView revealing a track", .serialized)
struct AlbumGridRevealTests {
    @Observable
    @MainActor
    final class Stage {
        /// Whether the open album's detail has laid out its track rows, as
        /// it does once the album's release has loaded.
        var rowsShown = true
        /// Where each track row sits in the visible area.
        @ObservationIgnored
        var rowFrames: [String: CGRect] = [:]
        /// The track flashed, and where its row sat when the flash came.
        @ObservationIgnored
        var flashed: (trackId: String, frame: CGRect?)?
    }

    /// Thirty 40-point track rows, `<album>-t0` to `<album>-t29`, in place
    /// of the album's detail.
    struct TrackRows: View {
        @Environment(UiStore.self)
        private var uiStore
        let albumId: String
        let stage: Stage

        var body: some View {
            VStack(spacing: 0) {
                if stage.rowsShown {
                    ForEach(0..<30, id: \.self) { index in
                        row("\(albumId)-t\(index)")
                    }
                }
                else {
                    Color.clear.frame(height: 200)
                }
            }
            .onChange(of: uiStore.pendingTrackFlash?.seq) {
                if let flash = uiStore.pendingTrackFlash {
                    stage.flashed = (
                        flash.trackId, stage.rowFrames[flash.trackId]
                    )
                }
            }
        }

        private func row(_ trackId: String) -> some View {
            Text(verbatim: trackId)
                .frame(maxWidth: .infinity, minHeight: 40, maxHeight: 40)
                .revealsAsTrackRow(trackId)
                .onGeometryChange(for: CGRect.self) { geometry in
                    geometry.frame(in: .scrollView)
                } action: { frame in
                    stage.rowFrames[trackId] = frame
                }
        }
    }

    /// The grid with its scrolls made at once: the test host's window is
    /// off screen, where SwiftUI never advances an animated scroll.
    struct Harness: View {
        let stage: Stage
        let list: AlbumList

        var body: some View {
            AlbumGridView(
                list: list,
                sortCriteria: [],
                fullWidth: false,
                selection: AlbumGridSelection(),
                onPlay: { _ in },
                onAddToQueue: { _ in },
                onAddNext: { _ in },
                expansionContent: { albumId in
                    TrackRows(albumId: albumId, stage: stage)
                }
            )
            .transaction { $0.disablesAnimations = true }
        }
    }

    private func host(
        _ stage: Stage,
        preloaded: Bool,
        openAlbumId: String? = nil
    ) async -> HostedAlbumGrid {
        await HostedAlbumGrid.host(
            preloaded: preloaded,
            openAlbumId: openAlbumId
        ) { list in
            Harness(stage: stage, list: list)
        }
    }

    /// Whether `frame`, in the visible area, is wholly inside it.
    private func inView(_ frame: CGRect?, of hosted: HostedAlbumGrid) throws
        -> Bool
    {
        let frame = try #require(frame)
        let height = try #require(hosted.scrollView).contentView.bounds.height
        return frame.minY > -0.5 && frame.maxY < height + 0.5
    }

    @Test("a track far down the grid is scrolled into view, then flashed")
    func farTrackScrolledIntoView() async throws {
        let stage = Stage()
        let hosted = await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300", trackId: "grid-300-t28")
        await hosted.settle()

        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(hosted.uiStore.pendingTrackFlash?.trackId == "grid-300-t28")
        #expect(try inView(stage.rowFrames["grid-300-t28"], of: hosted))
        #expect(stage.flashed?.trackId == "grid-300-t28")
        #expect(try inView(stage.flashed?.frame, of: hosted))
    }

    @Test("a track already in view is flashed where it is")
    func trackInViewFlashedInPlace() async throws {
        let stage = Stage()
        let hosted = await host(stage, preloaded: true, openAlbumId: "grid-0")
        defer { hosted.window.close() }
        let scrollView = try #require(hosted.scrollView)
        #expect(try inView(stage.rowFrames["grid-0-t1"], of: hosted))

        hosted.uiStore.navigateToAlbum("grid-0", trackId: "grid-0-t1")
        await hosted.settle()

        #expect(scrollView.contentView.bounds.origin.y == 0)
        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(stage.flashed?.trackId == "grid-0-t1")
    }

    @Test("a track below the fold of an open album is scrolled into view")
    func openAlbumTrackScrolledIntoView() async throws {
        let stage = Stage()
        let hosted = await host(stage, preloaded: true, openAlbumId: "grid-0")
        defer { hosted.window.close() }
        #expect(try !inView(stage.rowFrames["grid-0-t28"], of: hosted))

        hosted.uiStore.navigateToAlbum("grid-0", trackId: "grid-0-t28")
        await hosted.settle()

        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(try inView(stage.rowFrames["grid-0-t28"], of: hosted))
        #expect(stage.flashed?.trackId == "grid-0-t28")
        #expect(try inView(stage.flashed?.frame, of: hosted))
    }

    @Test("the reveal waits for the track's row to be laid out")
    func revealWaitsForRow() async throws {
        let stage = Stage()
        stage.rowsShown = false
        let hosted = await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300", trackId: "grid-300-t28")
        await hosted.settle()
        #expect(hosted.uiStore.pendingAlbumReveal?.trackId == "grid-300-t28")
        #expect(stage.flashed == nil)

        stage.rowsShown = true
        await hosted.settle()
        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(try inView(stage.rowFrames["grid-300-t28"], of: hosted))
        #expect(stage.flashed?.trackId == "grid-300-t28")
        #expect(try inView(stage.flashed?.frame, of: hosted))
    }

    @Test("closing the album gives up a reveal still waiting for its row")
    func closingAlbumEndsReveal() async throws {
        let stage = Stage()
        stage.rowsShown = false
        let hosted = await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300", trackId: "grid-300-t28")
        await hosted.settle()
        hosted.uiStore.closeAlbumDetail()
        await hosted.settle()
        #expect(hosted.uiStore.pendingAlbumReveal == nil)

        // Opening the album again later flashes nothing.
        hosted.uiStore.selectAlbum("grid-300")
        stage.rowsShown = true
        await hosted.settle()
        #expect(stage.flashed == nil)
        #expect(hosted.uiStore.pendingTrackFlash == nil)
    }

    @Test("an album revealed alone ends its reveal with nothing flashed")
    func albumAloneFlashesNothing() async throws {
        let stage = Stage()
        let hosted = await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300")
        await hosted.settle()

        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(hosted.uiStore.pendingTrackFlash == nil)
        #expect(stage.rowFrames["grid-300-t0"] != nil)
    }
}
