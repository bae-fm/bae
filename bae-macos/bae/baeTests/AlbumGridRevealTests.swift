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
    /// Measure the flash and frame together; separate callbacks can pair
    /// a new flash with a position from the previous layout.
    struct Placement: Equatable {
        let trackId: String
        let frame: CGRect
        let flashSeq: Int?
    }

    @Observable
    @MainActor
    final class Stage {
        /// Whether the open album's detail has laid out its track rows, as
        /// it does once the album's release has loaded.
        var rowsShown = true
        /// Where each track row sits in the visible area.
        var rowFrames: [String: CGRect] = [:]
        /// The track flashed, and where its row sat when the flash came.
        var flashed: Placement?
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
        }

        private func row(_ trackId: String) -> some View {
            Text(verbatim: trackId)
                .frame(maxWidth: .infinity, minHeight: 40, maxHeight: 40)
                .revealsAsTrackRow(trackId)
                .onGeometryChange(for: Placement.self) { geometry in
                    Placement(
                        trackId: trackId,
                        frame: geometry.frame(in: .scrollView),
                        flashSeq: uiStore.pendingTrackFlash.flatMap {
                            $0.trackId == trackId ? $0.seq : nil
                        }
                    )
                } action: { placement in
                    stage.rowFrames[trackId] = placement.frame
                    if let seq = placement.flashSeq,
                        stage.flashed?.flashSeq != seq
                    {
                        stage.flashed = placement
                    }
                }
        }
    }

    /// The grid with its scrolls made at once: the test host's window is
    /// off screen, where SwiftUI never advances an animated scroll.
    struct Harness: View {
        let stage: Stage
        let list: AlbumList
        var groupByArtist = false

        var body: some View {
            AlbumGridView(
                list: list,
                sortCriteria: [],
                groupByArtist: groupByArtist,
                fullWidth: false,
                selection: AlbumGridSelection(),
                onPlay: { _ in },
                onAddToQueue: { _ in },
                onAddNext: { _ in },
                onDeleteAlbum: { _ in },
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
    ) async throws -> HostedAlbumGrid {
        try await HostedAlbumGrid.host(
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

    /// Return once the reveal of `trackId` has ended with the track flashed,
    /// and the grid has come to rest.
    private func revealed(
        _ trackId: String,
        in hosted: HostedAlbumGrid,
        _ stage: Stage
    ) async throws {
        try await Wait.until {
            hosted.uiStore.pendingAlbumReveal == nil
                && stage.flashed?.trackId == trackId
        }
        try await hosted.settle()
    }

    @Test("a track in another artist's grid is revealed beneath its album row")
    func groupedTrackScrolledIntoView() async throws {
        let stage = Stage()
        let sections = [
            BridgeLibraryBrowseSection(
                id: "artist-a",
                title: "Artist A",
                image: nil,
                window: .init(offset: 0, limit: 203)
            ),
            BridgeLibraryBrowseSection(
                id: "artist-b",
                title: "Artist B",
                image: nil,
                window: .init(offset: 203, limit: 202)
            ),
            BridgeLibraryBrowseSection(
                id: "artist-c",
                title: "Artist C",
                image: nil,
                window: .init(offset: 405, limit: 195)
            ),
        ]
        let hosted = try await HostedAlbumGrid.host(
            preloaded: false,
            sections: sections
        ) { list in
            Harness(stage: stage, list: list, groupByArtist: true)
        }
        defer { hosted.window.close() }
        hosted.uiStore.navigateToAlbum("grid-404", trackId: "grid-404-t28")
        try await revealed("grid-404-t28", in: hosted, stage)
        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(stage.flashed?.trackId == "grid-404-t28")
        #expect(try inView(stage.rowFrames["grid-404-t28"], of: hosted))
    }

    @Test("a track far down the grid is scrolled into view, then flashed")
    func farTrackScrolledIntoView() async throws {
        let stage = Stage()
        let hosted = try await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300", trackId: "grid-300-t28")
        try await revealed("grid-300-t28", in: hosted, stage)

        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(hosted.uiStore.pendingTrackFlash?.trackId == "grid-300-t28")
        #expect(try inView(stage.rowFrames["grid-300-t28"], of: hosted))
        #expect(stage.flashed?.trackId == "grid-300-t28")
        #expect(try inView(stage.flashed?.frame, of: hosted))
    }

    @Test("a track already in view is flashed where it is")
    func trackInViewFlashedInPlace() async throws {
        let stage = Stage()
        let hosted = try await host(
            stage,
            preloaded: true,
            openAlbumId: "grid-0"
        )
        defer { hosted.window.close() }
        let scrollView = try #require(hosted.scrollView)
        #expect(try inView(stage.rowFrames["grid-0-t1"], of: hosted))

        hosted.uiStore.navigateToAlbum("grid-0", trackId: "grid-0-t1")
        try await revealed("grid-0-t1", in: hosted, stage)

        #expect(scrollView.contentView.bounds.origin.y == 0)
        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(stage.flashed?.trackId == "grid-0-t1")
    }

    @Test("a track below the fold of an open album is scrolled into view")
    func openAlbumTrackScrolledIntoView() async throws {
        let stage = Stage()
        let hosted = try await host(
            stage,
            preloaded: true,
            openAlbumId: "grid-0"
        )
        defer { hosted.window.close() }
        #expect(try !inView(stage.rowFrames["grid-0-t28"], of: hosted))

        hosted.uiStore.navigateToAlbum("grid-0", trackId: "grid-0-t28")
        try await revealed("grid-0-t28", in: hosted, stage)

        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(try inView(stage.rowFrames["grid-0-t28"], of: hosted))
        #expect(stage.flashed?.trackId == "grid-0-t28")
        #expect(try inView(stage.flashed?.frame, of: hosted))
    }

    @Test("the reveal waits for the track's row to be laid out")
    func revealWaitsForRow() async throws {
        let stage = Stage()
        stage.rowsShown = false
        let hosted = try await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300", trackId: "grid-300-t28")
        try await hosted.untilScrolled()
        try await hosted.settle()
        #expect(hosted.uiStore.pendingAlbumReveal?.trackId == "grid-300-t28")
        #expect(stage.flashed == nil)

        stage.rowsShown = true
        try await revealed("grid-300-t28", in: hosted, stage)
        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(try inView(stage.rowFrames["grid-300-t28"], of: hosted))
        #expect(stage.flashed?.trackId == "grid-300-t28")
        #expect(try inView(stage.flashed?.frame, of: hosted))
    }

    @Test("closing the album gives up a reveal still waiting for its row")
    func closingAlbumEndsReveal() async throws {
        let stage = Stage()
        stage.rowsShown = false
        let hosted = try await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300", trackId: "grid-300-t28")
        try await hosted.untilScrolled()
        try await hosted.settle()
        hosted.uiStore.closeAlbumDetail()
        try await hosted.settle()
        #expect(hosted.uiStore.pendingAlbumReveal == nil)

        // Opening the album again later flashes nothing.
        hosted.uiStore.selectAlbum("grid-300")
        stage.rowsShown = true
        try await Wait.until { stage.rowFrames["grid-300-t29"] != nil }
        #expect(stage.flashed == nil)
        #expect(hosted.uiStore.pendingTrackFlash == nil)
    }

    @Test("an album revealed alone ends its reveal with nothing flashed")
    func albumAloneFlashesNothing() async throws {
        let stage = Stage()
        let hosted = try await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300")
        try await Wait.until {
            hosted.uiStore.pendingAlbumReveal == nil
                && stage.rowFrames["grid-300-t0"] != nil
        }

        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(hosted.uiStore.pendingTrackFlash == nil)
        #expect(stage.rowFrames["grid-300-t0"] != nil)
    }
}
