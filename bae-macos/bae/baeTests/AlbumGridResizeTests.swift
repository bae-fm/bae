import AppKit
import BaeKit
import Observation
import SwiftUI
import Testing

@testable import bae

/// The grid hosted in a window beside a panel that springs in and out, as the
/// queue does in `MainAppView`, so a width change crosses a column count.
@MainActor
@Suite("AlbumGridView across a column change", .serialized)
struct AlbumGridResizeTests {
    @Observable
    @MainActor
    final class Stage {
        var showPanel = false
        /// Where the open album's detail sits in the visible area.
        @ObservationIgnored
        var detailTop: CGFloat?
    }

    struct Harness: View {
        let stage: Stage
        let list: AlbumList

        var body: some View {
            HStack(spacing: 0) {
                AlbumGridView(
                    list: list,
                    sortCriteria: [],
                    fullWidth: false,
                    selection: AlbumGridSelection(),
                    onPlay: { _ in },
                    onAddToQueue: { _ in },
                    onAddNext: { _ in }
                ) { albumId in
                    Text(verbatim: "Detail \(albumId)")
                        .frame(maxWidth: .infinity, minHeight: 240)
                        .onGeometryChange(for: CGFloat.self) { geometry in
                            geometry.frame(in: .scrollView).minY
                        } action: { top in
                            stage.detailTop = top
                        }
                }
                .frame(maxWidth: .infinity)
                if stage.showPanel {
                    Color.gray.frame(width: 320)
                        .transition(.move(edge: .trailing))
                }
            }
            .animation(
                .spring(duration: 0.24, bounce: 0.12),
                value: stage.showPanel
            )
        }
    }

    /// The 1180-point window holds five columns; with the panel beside it,
    /// three.
    private static let wideColumns = 5.0

    /// How the grid stands when the panel comes and goes.
    struct Scene: Sendable, CustomTestStringConvertible {
        /// Every page loaded up front, or each page loading as its rows show.
        let preloaded: Bool
        let openAlbumId: String?
        let scrolledTo: CGFloat

        var testDescription: String {
            "preloaded \(preloaded), open \(openAlbumId ?? "none"), at \(scrolledTo)"
        }
    }

    @Test(
        "a column change keeps the slot on top on top",
        arguments: [
            Scene(preloaded: false, openAlbumId: nil, scrolledTo: 6000),
            Scene(preloaded: true, openAlbumId: nil, scrolledTo: 6000),
            // The detail under the row holding album 100, the row just under
            // the top.
            Scene(preloaded: false, openAlbumId: "grid-100", scrolledTo: 6000),
            // The same detail partly scrolled past, so it is the slot on top.
            Scene(preloaded: true, openAlbumId: "grid-100", scrolledTo: 6450),
        ]
    )
    func columnChangeKeepsTopSlot(scene: Scene) async throws {
        let stage = Stage()
        let hosted = await HostedAlbumGrid.host(
            preloaded: scene.preloaded,
            openAlbumId: scene.openAlbumId
        ) { list in
            Harness(stage: stage, list: list)
        }
        defer { hosted.window.close() }
        let scrollView = try #require(hosted.scrollView)
        let offset = { scrollView.contentView.bounds.origin.y }
        // With a detail open the grid places the rows it has not drawn by
        // estimate, so the offset is no measure; where the detail sits is.
        let place = {
            scene.openAlbumId == nil ? offset() : stage.detailTop ?? .nan
        }

        scrollView.contentView.scroll(to: NSPoint(x: 0, y: scene.scrolledTo))
        scrollView.reflectScrolledClipView(scrollView.contentView)
        await hosted.settle()
        let before = place()
        let content = try #require(scrollView.documentView).frame.height
        let rowPitch =
            content
            / (Double(HostedAlbumGrid.albums.count) / Self.wideColumns)
            .rounded(.up)

        stage.showPanel = true
        await hosted.settle()
        let narrowed = place()
        if scene.openAlbumId == nil {
            // Fewer columns put the slot on top further down the content.
            #expect(narrowed > before)
        }

        stage.showPanel = false
        await hosted.settle()
        let back = place()
        // Back at five columns the slot on top is the one that was: the grid
        // moved by less than half a row, to line that slot up with the top.
        #expect(abs(back - before) <= rowPitch / 2)

        // Going back and forth again returns to the same places.
        stage.showPanel = true
        await hosted.settle()
        #expect(abs(place() - narrowed) < 1)
        stage.showPanel = false
        await hosted.settle()
        #expect(abs(place() - back) < 1)

        if scene.scrolledTo == 6450 {
            // The detail was the slot on top, and stays there.
            #expect(abs(narrowed) < 1)
            #expect(abs(back) < 1)
        }
    }
}
