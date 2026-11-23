import AppKit
import BaeKit
import Combine
import Observation
import SwiftUI
import Synchronization
import Testing

@testable import bae

/// The grid revealing an album, and a track in the album's detail, as Go to
/// Now Playing and a track picked in search ask it to.
@MainActor
@Suite("MacAlbumGrid revealing a track", .serialized)
struct AlbumGridRevealTests {
    /// Measure the flash and frame together; separate callbacks can pair
    /// a new flash with a position from the previous layout.
    struct Placement: Equatable {
        let trackId: String
        let frame: CGRect
        /// How many flashes the row had received when it sat there.
        let flashes: Int
    }

    @Observable
    @MainActor
    final class Stage {
        /// Whether the open album's detail has laid out its track rows, as
        /// it does once the album's release has loaded.
        var rowsShown = true
        /// Where each track row sits in the visible area.
        var rowFrames: [String: CGRect] = [:]
        /// The track the grid tells the open album's detail it is revealing,
        /// as the detail last saw it.
        var revealing: String?
        /// How many flashes each track's row received.
        var flashes: [String: Int] = [:]
        /// The last track flashed, and where its row sat when the flash came.
        var flashed: Placement?

        /// How many flashes the rows received in all.
        var flashCount: Int {
            flashes.values.reduce(0, +)
        }
    }

    /// Thirty 40-point track rows, `<album>-t0` to `<album>-t29`, in place
    /// of the album's detail.
    struct TrackRows: View {
        @Environment(UiStore.self)
        private var uiStore
        @Environment(\.revealedTrackId)
        private var revealedTrackId
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
            .onChange(of: revealedTrackId, initial: true) { _, trackId in
                stage.revealing = trackId
            }
        }

        private func row(_ trackId: String) -> some View {
            let flashes = stage.flashes[trackId] ?? 0
            return Text(verbatim: trackId)
                .frame(maxWidth: .infinity, minHeight: 40, maxHeight: 40)
                .revealsAsTrackRow(trackId)
                .onGeometryChange(for: Placement.self) { geometry in
                    Placement(
                        trackId: trackId,
                        frame: geometry.frame(in: .scrollView),
                        flashes: flashes
                    )
                } action: { placement in
                    stage.rowFrames[trackId] = placement.frame
                    if placement.flashes > 0,
                        stage.flashed?.trackId != trackId
                            || stage.flashed?.flashes != placement.flashes
                    {
                        stage.flashed = placement
                    }
                }
                .onReceive(uiStore.trackFlashes) { flashed in
                    if flashed == trackId {
                        stage.flashes[trackId] = flashes + 1
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
            MacAlbumGrid(
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
            stage.revealing == nil && stage.flashed?.trackId == trackId
        }
        try await hosted.settle()
    }

    @Test("a track in another artist's grid is revealed beneath its album row")
    func groupedTrackScrolledIntoView() async throws {
        let stage = Stage()
        let requestedSections = Mutex<[String?]>([])
        let recordSection: @Sendable (String?) -> Void = { sectionId in
            requestedSections.withLock { $0.append(sectionId) }
        }
        let sections = [
            .fixture(
                "artist-a",
                title: "Artist A",
                firstPosition: 0,
                albumCount: 203
            ),
            .fixture(
                "artist-b",
                title: "Artist B",
                firstPosition: 203,
                albumCount: 202
            ),
            .fixture(
                "artist-c",
                title: "Artist C",
                firstPosition: 405,
                albumCount: 195
            ),
        ]
        let hosted = try await HostedAlbumGrid.host(
            preloaded: false,
            sections: sections,
            onPlaceRead: recordSection
        ) { list in
            Harness(stage: stage, list: list, groupByArtist: true)
        }
        defer { hosted.window.close() }
        hosted.uiStore.navigateToAlbum("grid-404", trackId: "grid-404-t28")
        try await revealed("grid-404-t28", in: hosted, stage)
        #expect(stage.flashCount == 1)
        #expect(try inView(stage.rowFrames["grid-404-t28"], of: hosted))
        #expect(requestedSections.withLock { $0 } == [nil])
    }

    @Test("a track far down the grid is scrolled into view, then flashed")
    func farTrackScrolledIntoView() async throws {
        let stage = Stage()
        let hosted = try await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300", trackId: "grid-300-t28")
        try await revealed("grid-300-t28", in: hosted, stage)

        #expect(try inView(stage.rowFrames["grid-300-t28"], of: hosted))
        #expect(stage.flashCount == 1)
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
        #expect(stage.flashCount == 1)
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

        #expect(try inView(stage.rowFrames["grid-0-t28"], of: hosted))
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
        #expect(stage.flashed == nil)

        stage.rowsShown = true
        try await revealed("grid-300-t28", in: hosted, stage)
        #expect(try inView(stage.rowFrames["grid-300-t28"], of: hosted))
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
        #expect(stage.revealing == "grid-300-t28")
        hosted.uiStore.closeAlbumDetail()
        try await hosted.settle()

        // Opening the album again later reveals and flashes nothing.
        hosted.uiStore.selectAlbum("grid-300")
        stage.rowsShown = true
        try await Wait.until { stage.rowFrames["grid-300-t29"] != nil }
        try await hosted.settle()
        #expect(stage.revealing == nil)
        #expect(stage.flashed == nil)
    }

    @Test("an album revealed alone is scrolled to with nothing flashed")
    func albumAloneFlashesNothing() async throws {
        let stage = Stage()
        let hosted = try await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300")
        try await Wait.until { stage.rowFrames["grid-300-t0"] != nil }
        try await hosted.settle()

        #expect(stage.flashed == nil)
    }
}

extension AlbumGridRevealTests {
    @Test("asking again for a track already shown reveals it again")
    func repeatRevealFlashesAgain() async throws {
        let stage = Stage()
        let hosted = try await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300", trackId: "grid-300-t28")
        try await revealed("grid-300-t28", in: hosted, stage)
        hosted.uiStore.navigateToAlbum("grid-300", trackId: "grid-300-t28")
        try await Wait.until { stage.flashed?.flashes == 2 }
        try await hosted.settle()

        #expect(stage.revealing == nil)
        #expect(stage.flashCount == 2)
        #expect(try inView(stage.flashed?.frame, of: hosted))
    }

    @Test("a reveal of an album the grid has no card of ends where it is")
    func albumWithNoCardEndsReveal() async throws {
        let stage = Stage()
        let hosted = try await host(stage, preloaded: false)
        defer { hosted.window.close() }
        let top = try #require(hosted.scrollView).contentView.bounds.origin

        hosted.uiStore.navigateToAlbum("no-such-album")
        try await hosted.settle()

        #expect(
            try #require(hosted.scrollView).contentView.bounds.origin == top
        )
        #expect(stage.flashed == nil)
    }
}
