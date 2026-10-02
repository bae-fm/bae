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
    private func host(
        _ stage: RevealStage,
        preloaded: Bool,
        openAlbumId: String? = nil
    ) async -> HostedAlbumGrid {
        await HostedAlbumGrid.host(
            preloaded: preloaded,
            openAlbumId: openAlbumId
        ) { list in
            RevealHarness(stage: stage, list: list)
        }
    }

    /// The height of the grid's visible area.
    private func visibleHeight(_ hosted: HostedAlbumGrid) throws -> CGFloat {
        try #require(hosted.scrollView).contentView.bounds.height
    }

    /// Whether `frame`, in the visible area, is wholly inside it.
    private func inView(_ frame: CGRect?, of hosted: HostedAlbumGrid) throws
        -> Bool
    {
        guard let frame else { return false }
        return try frame.minY > -0.5 && frame.maxY < visibleHeight(hosted) + 0.5
    }

    /// Whether `frame`, in the visible area, is at least partly inside it.
    private func partlyInView(_ frame: CGRect?, of hosted: HostedAlbumGrid)
        throws -> Bool
    {
        guard let frame else { return false }
        return try frame.maxY > 0 && frame.minY < visibleHeight(hosted)
    }

    @Test("a track in another artist's grid is revealed beneath its album row")
    func groupedTrackScrolledIntoView() async throws {
        let stage = RevealStage()
        let sections = [
            BridgeLibraryBrowseSection(
                id: "artist-a",
                title: "Artist A",
                window: .init(offset: 0, limit: 203)
            ),
            BridgeLibraryBrowseSection(
                id: "artist-b",
                title: "Artist B",
                window: .init(offset: 203, limit: 202)
            ),
            BridgeLibraryBrowseSection(
                id: "artist-c",
                title: "Artist C",
                window: .init(offset: 405, limit: 195)
            ),
        ]
        let hosted = await HostedAlbumGrid.host(
            preloaded: false,
            sections: sections
        ) { list in
            RevealHarness(stage: stage, list: list, groupByArtist: true)
        }
        defer { hosted.window.close() }
        hosted.uiStore.navigateToAlbum("grid-404", trackId: "grid-404-t28")
        await hosted.waitUntil("the track flashes") {
            stage.flashed?.trackId == "grid-404-t28"
        }
        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(try inView(stage.rowFrames["grid-404-t28"], of: hosted))
    }

    @Test("a track far down the grid is scrolled into view, then flashed")
    func farTrackScrolledIntoView() async throws {
        let stage = RevealStage()
        let hosted = await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300", trackId: "grid-300-t28")
        await hosted.waitUntil("the track flashes") { stage.flashed != nil }

        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(hosted.uiStore.pendingTrackFlash?.trackId == "grid-300-t28")
        #expect(try inView(stage.rowFrames["grid-300-t28"], of: hosted))
        #expect(stage.flashed?.trackId == "grid-300-t28")
        #expect(try inView(stage.flashed?.frame, of: hosted))
    }

    @Test("a track already in view is flashed where it is")
    func trackInViewFlashedInPlace() async throws {
        let stage = RevealStage()
        let hosted = await host(stage, preloaded: true, openAlbumId: "grid-0")
        defer { hosted.window.close() }
        let scrollView = try #require(hosted.scrollView)
        await hosted.waitUntil("the open album's rows are laid out") {
            stage.rowFrames["grid-0-t1"] != nil
        }
        #expect(try inView(stage.rowFrames["grid-0-t1"], of: hosted))

        hosted.uiStore.navigateToAlbum("grid-0", trackId: "grid-0-t1")
        await hosted.waitUntil("the track flashes") { stage.flashed != nil }

        #expect(scrollView.contentView.bounds.origin.y == 0)
        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(stage.flashed?.trackId == "grid-0-t1")
    }

    @Test("a track below the fold of an open album is scrolled into view")
    func openAlbumTrackScrolledIntoView() async throws {
        let stage = RevealStage()
        let hosted = await host(stage, preloaded: true, openAlbumId: "grid-0")
        defer { hosted.window.close() }
        await hosted.waitUntil("the open album's rows are laid out") {
            stage.rowFrames["grid-0-t28"] != nil
        }
        #expect(try !inView(stage.rowFrames["grid-0-t28"], of: hosted))

        hosted.uiStore.navigateToAlbum("grid-0", trackId: "grid-0-t28")
        await hosted.waitUntil("the track flashes") { stage.flashed != nil }

        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(try inView(stage.rowFrames["grid-0-t28"], of: hosted))
        #expect(stage.flashed?.trackId == "grid-0-t28")
        #expect(try inView(stage.flashed?.frame, of: hosted))
    }

    @Test("the reveal waits for the track's row to be laid out")
    func revealWaitsForRow() async throws {
        let stage = RevealStage()
        stage.rowsShown = false
        let hosted = await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300", trackId: "grid-300-t28")
        await hosted.waitUntil("the grid is at the album, its detail loading") {
            (try? partlyInView(stage.standInFrame, of: hosted)) == true
        }
        #expect(hosted.uiStore.pendingAlbumReveal?.trackId == "grid-300-t28")
        #expect(stage.flashed == nil)

        stage.rowsShown = true
        await hosted.waitUntil("the track flashes") { stage.flashed != nil }
        #expect(hosted.uiStore.pendingAlbumReveal == nil)
        #expect(try inView(stage.rowFrames["grid-300-t28"], of: hosted))
        #expect(stage.flashed?.trackId == "grid-300-t28")
        #expect(try inView(stage.flashed?.frame, of: hosted))
    }

    @Test("closing the album gives up a reveal still waiting for its row")
    func closingAlbumEndsReveal() async throws {
        let stage = RevealStage()
        stage.rowsShown = false
        let hosted = await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300", trackId: "grid-300-t28")
        await hosted.waitUntil("the grid is at the album, its detail loading") {
            (try? partlyInView(stage.standInFrame, of: hosted)) == true
        }
        hosted.uiStore.closeAlbumDetail()
        await hosted.waitUntil("the reveal ends") {
            hosted.uiStore.pendingAlbumReveal == nil
        }

        // Opening the album again later flashes nothing.
        hosted.uiStore.selectAlbum("grid-300")
        stage.rowsShown = true
        await hosted.waitUntil("the album's rows are laid out again") {
            stage.rowFrames["grid-300-t28"] != nil
        }
        #expect(stage.flashed == nil)
        #expect(hosted.uiStore.pendingTrackFlash == nil)
    }

    @Test("an album revealed alone ends its reveal with nothing flashed")
    func albumAloneFlashesNothing() async throws {
        let stage = RevealStage()
        let hosted = await host(stage, preloaded: false)
        defer { hosted.window.close() }

        hosted.uiStore.navigateToAlbum("grid-300")
        await hosted.waitUntil("the reveal ends") {
            hosted.uiStore.pendingAlbumReveal == nil
        }

        #expect(hosted.uiStore.pendingTrackFlash == nil)
        #expect(stage.rowFrames["grid-300-t0"] != nil)
    }

    @Test(
        "an album in the last row is shown with its detail whole once it loads"
    )
    func lastRowAlbumShowsItsDetail() async throws {
        let stage = RevealStage()
        stage.rowsShown = false
        // The detail loads taller than it stands in, and with its card fits
        // in view, so the content ends before the card reaches the top.
        stage.standInHeight = 40
        stage.rowCount = 8
        let hosted = await host(stage, preloaded: false)
        defer { hosted.window.close() }
        let scrollView = try #require(hosted.scrollView)

        hosted.uiStore.navigateToAlbum("grid-599")
        await hosted.waitUntil("the grid is at the album, its detail loading") {
            (try? partlyInView(stage.standInFrame, of: hosted)) == true
        }
        // Where the detail goes is not known while it stands in.
        #expect(hosted.uiStore.pendingAlbumReveal?.albumId == "grid-599")

        stage.rowsShown = true
        await hosted.waitUntil("the reveal ends") {
            hosted.uiStore.pendingAlbumReveal == nil
        }
        await hosted.waitUntil("the detail's last row is in view") {
            (try? inView(stage.rowFrames["grid-599-t7"], of: hosted)) == true
        }
        // As near the top as the content scrolls: its end.
        let content = try #require(scrollView.documentView).frame.height
        #expect(abs(scrollView.contentView.bounds.maxY - content) < 0.5)
    }
}

/// A flash, and where its row sat when the flash came.
private struct RevealPlacement: Equatable {
    let trackId: String
    let frame: CGRect?
    let flashSeq: Int
}

@Observable
@MainActor
private final class RevealStage {
    /// Whether the open album's detail has laid out its track rows, as
    /// it does once the album's release has loaded.
    var rowsShown = true
    /// How many track rows the detail lays out.
    @ObservationIgnored
    var rowCount = 30
    /// The height of what the detail shows until its rows are laid out.
    @ObservationIgnored
    var standInHeight: CGFloat = 200
    /// Where each track row sits in the visible area.
    @ObservationIgnored
    var rowFrames: [String: CGRect] = [:]
    /// Where the detail's stand-in sits in the visible area while it
    /// shows.
    @ObservationIgnored
    var standInFrame: CGRect?
    /// The track flashed, and where its row sat when the flash came.
    @ObservationIgnored
    var flashed: RevealPlacement?
}

/// `stage.rowCount` 40-point track rows, `<album>-t0` on, in place of
/// the album's detail, once `stage.rowsShown`.
private struct RevealTrackRows: View {
    @Environment(UiStore.self)
    private var uiStore
    let albumId: String
    let stage: RevealStage

    var body: some View {
        if stage.rowsShown {
            VStack(spacing: 0) {
                ForEach(0..<stage.rowCount, id: \.self) { index in
                    row("\(albumId)-t\(index)")
                }
            }
            .revealsAsAlbumDetail(albumId)
        }
        else {
            Color.clear.frame(height: stage.standInHeight)
                .onGeometryChange(for: CGRect.self) { geometry in
                    geometry.frame(in: .scrollView)
                } action: { frame in
                    stage.standInFrame = frame
                }
                .onDisappear { stage.standInFrame = nil }
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
            // As the app's track row takes its flash.
            .onChange(of: uiStore.pendingTrackFlash?.seq, initial: true) {
                guard let flash = uiStore.pendingTrackFlash,
                    flash.trackId == trackId,
                    stage.flashed?.flashSeq != flash.seq
                else { return }
                stage.flashed = RevealPlacement(
                    trackId: trackId,
                    frame: stage.rowFrames[trackId],
                    flashSeq: flash.seq
                )
            }
    }
}

/// The grid with its animations off: the test host's window is off
/// screen, where SwiftUI never advances an animation.
private struct RevealHarness: View {
    let stage: RevealStage
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
            expansionContent: { albumId in
                RevealTrackRows(albumId: albumId, stage: stage)
            }
        )
        .transaction { $0.disablesAnimations = true }
    }
}
