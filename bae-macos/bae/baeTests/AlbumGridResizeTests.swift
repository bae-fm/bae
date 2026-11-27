import AppKit
import BaeKit
import Observation
import SwiftUI
import Testing

@testable import bae

/// The grid hosted in a window beside a panel that springs in and out, as the
/// queue does in `MacMainWindowContainer`, so a width change crosses a column count.
@MainActor
@Suite("MacAlbumGrid across a column change", .serialized)
struct AlbumGridResizeTests {
    @Observable
    @MainActor
    final class Stage {
        private(set) var showPanel = false
        /// How far the spring has carried the panel in, as SwiftUI draws
        /// each of its frames: 0 out, 1 in.
        var panelIn = 0.0
        /// Where the open album's detail sits in the visible area, once it
        /// has reported.
        var detailTop: CGFloat?

        /// Show or hide the panel, and return once its spring is over, the
        /// row on top is drawn from its rows and the grid has come to rest.
        ///
        /// Between the spring's frames the run loop idles with every view
        /// where the last frame left it, so mid-spring the grid reads as at
        /// rest. The spring is over when the value it carries lands on its
        /// end; until then each frame has it short of the end or past it.
        func movePanel(
            show: Bool,
            in hosted: HostedAlbumGrid,
            list: AlbumList,
            detailOpen: Bool
        ) async throws {
            showPanel = show
            let end = show ? 1.0 : 0.0
            try await Wait.until { self.panelIn == end }
            try await comeToRest(
                columns: show ? Self.narrowColumns : Self.wideColumns,
                in: hosted,
                list: list,
                detailOpen: detailOpen
            )
        }

        /// Return once the row holding the album the scenes center on is
        /// drawn from its rows, not placeholders, and the detail under it, if
        /// open, has said where it is.
        ///
        /// A row shows placeholders until the page holding its albums lands,
        /// and the page is asked for from a task each cell starts a few turns
        /// after the layout that made the cell, so a grid with nothing unanswered
        /// and its frames still can be a turn from asking. A placeholder row is
        /// shorter than one of cards, so reading the detail's place before
        /// the rows land reads a place the grid then leaves.
        func comeToRest(
            columns: Int,
            in hosted: HostedAlbumGrid,
            list: AlbumList,
            detailOpen: Bool
        ) async throws {
            let first = Self.centeredPosition / columns * columns
            try await Wait.until {
                (first..<first + columns).allSatisfy { list.row(at: $0) != nil }
            }
            if detailOpen {
                try await Wait.until { self.detailTop != nil }
            }
            try await hosted.settle { self.detailTop }
        }

        /// Scroll the open detail's top `past` points above the top of the
        /// visible area, by where the detail reports itself, and return once
        /// the grid has come to rest. Does nothing for 0.
        func scrollDetailPast(
            _ past: CGFloat,
            in hosted: HostedAlbumGrid,
            list: AlbumList
        ) async throws {
            guard past > 0 else { return }
            let clip = try #require(hosted.scrollView).contentView
            let detailTop = try #require(detailTop)
            clip.scroll(
                to: NSPoint(x: 0, y: clip.bounds.origin.y + detailTop + past)
            )
            hosted.scrollView?.reflectScrolledClipView(clip)
            try await comeToRest(
                columns: Self.wideColumns,
                in: hosted,
                list: list,
                detailOpen: true
            )
        }

        /// The position of the album the scenes center on: the one opened,
        /// and the first card of the row at the top of the scenes with none
        /// open.
        static let centeredPosition = 100
        /// The window holds five columns; with the panel beside it, three.
        static let wideColumns = 5
        static let narrowColumns = 3
    }

    /// Hands `report` each value the spring carrying the panel takes, so a
    /// test knows when the spring is over.
    private struct PanelSpring: ViewModifier, Animatable {
        var animatableData: Double
        let report: (Double) -> Void

        func body(content: Content) -> some View {
            content.onChange(of: animatableData) { _, value in
                report(value)
            }
        }
    }

    struct Harness: View {
        let stage: Stage
        let list: AlbumList

        var body: some View {
            HStack(spacing: 0) {
                MacAlbumGrid(
                    list: list,
                    sortCriteria: [],
                    fullWidth: false,
                    selection: AlbumGridSelection(),
                    onPlay: { _ in },
                    onAddToQueue: { _ in },
                    onAddNext: { _ in },
                    onDeleteAlbum: { _ in }
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
                // The detail of the album open at host time opens at once,
                // so no layout read lands partway through its spring; the
                // panel's spring still moves the grid.
                .transaction {
                    if $0.animation != QueuePanelMotion.spring {
                        $0.animation = nil
                        $0.disablesAnimations = true
                    }
                }
                if stage.showPanel {
                    Color.gray.frame(width: 320)
                        .transition(.move(edge: .trailing))
                }
            }
            .modifier(
                PanelSpring(animatableData: stage.showPanel ? 1 : 0) {
                    stage.panelIn = $0
                }
            )
            .animation(QueuePanelMotion.spring, value: stage.showPanel)
        }
    }

    /// An open album's detail, and how far the grid scrolls its top above the
    /// top of the visible area. The grid places the rows it has not drawn by
    /// estimate, so no offset puts the detail there; where the detail reports
    /// itself does.
    struct OpenDetail: Sendable {
        let albumId: String
        var scrolledPast: CGFloat = 0
    }

    /// How the grid stands when the panel comes and goes.
    struct Scene: Sendable, CustomTestStringConvertible {
        /// Every page loaded up front, or each page loading as its rows show.
        let preloaded: Bool
        let open: OpenDetail?
        let scrolledTo: CGFloat

        var testDescription: String {
            "preloaded \(preloaded), open \(open?.albumId ?? "none"), at \(scrolledTo), detail \(open?.scrolledPast ?? 0) past the top"
        }
    }

    @Test(
        "a column change keeps the slot on top",
        arguments: [
            // Album 100's row has more than half of itself in view, and the
            // row above less, wherever the lazy grid places them: it places
            // a row it jumps to within the grid's 32 point spacing of where
            // the rows above would put it.
            Scene(preloaded: false, open: nil, scrolledTo: 5900),
            Scene(preloaded: true, open: nil, scrolledTo: 5900),
            // The detail under the row holding album 100, which has less than
            // half of itself in view, so the detail is the slot on top.
            Scene(
                preloaded: false,
                open: OpenDetail(albumId: "grid-100"),
                scrolledTo: 6050
            ),
            // The same detail partly scrolled past, so it is the slot on top.
            Scene(
                preloaded: true,
                open: OpenDetail(albumId: "grid-100", scrolledPast: 60),
                scrolledTo: 6000
            ),
        ]
    )
    func columnChangeKeepsTopSlot(scene: Scene) async throws {
        let stage = Stage()
        var hostedList: AlbumList?
        let hosted = try await HostedAlbumGrid.host(
            preloaded: scene.preloaded,
            openAlbumId: scene.open?.albumId
        ) { list in
            hostedList = list
            return Harness(stage: stage, list: list)
        }
        let list = try #require(hostedList)
        let detailOpen = scene.open != nil
        let move = { (show: Bool) in
            try await stage.movePanel(
                show: show,
                in: hosted,
                list: list,
                detailOpen: detailOpen
            )
        }
        defer { hosted.window.close() }
        let scrollView = try #require(hosted.scrollView)
        let offset = { scrollView.contentView.bounds.origin.y }
        // With a detail open the grid places the rows it has not drawn by
        // estimate, so the offset is no measure; where the detail sits is.
        let place = {
            scene.open == nil ? offset() : stage.detailTop ?? .nan
        }

        scrollView.contentView.scroll(to: NSPoint(x: 0, y: scene.scrolledTo))
        scrollView.reflectScrolledClipView(scrollView.contentView)
        try await stage.comeToRest(
            columns: Stage.wideColumns,
            in: hosted,
            list: list,
            detailOpen: detailOpen
        )
        try await stage.scrollDetailPast(
            scene.open?.scrolledPast ?? 0,
            in: hosted,
            list: list
        )
        let before = place()
        let slotTop = {
            try await Self.slotTop(stage, in: hosted, detailOpen: detailOpen)
        }

        try await move(true)
        let narrowed = place()
        let narrowedTop = try await slotTop()
        #expect(abs(narrowedTop) < 1)
        if !detailOpen {
            // Fewer columns put the slot on top further down the content.
            #expect(narrowed > before)
        }

        try await move(false)
        let back = place()
        // Back at five columns the slot on top is the one that was.
        let backTop = try await slotTop()
        #expect(abs(backTop) < 1)

        // Going back and forth again returns to the same places.
        try await move(true)
        #expect(abs(place() - narrowed) < 1)
        try await move(false)
        #expect(abs(place() - back) < 1)
    }

    /// How far the slot a column change keeps sits below the top of the
    /// grid's visible area: the open detail, or else the card of the album
    /// the scenes center on.
    private static func slotTop(
        _ stage: Stage,
        in hosted: HostedAlbumGrid,
        detailOpen: Bool
    ) async throws -> Double {
        if detailOpen { return try #require(stage.detailTop) }
        let cardTop = { try centeredCardTop(in: hosted.window) }
        // The accessibility tree takes a layout's places some turns of the
        // run loop after the layout.
        try await Wait.untilSteady { try cardTop() }
        return try cardTop()
    }

    /// How far the card of the album the scenes center on sits below the top
    /// of the grid's visible area.
    private static func centeredCardTop(in window: NSWindow) throws -> Double {
        let tree = try AccessibilityTree.read(of: window, subject: "the grid")
        let visible = try #require(
            tree.nodes.first {
                $0.role == NSAccessibility.Role.scrollArea.rawValue
            }
        )
        let title = "Album Title \(Stage.centeredPosition)"
        let words = try #require(tree.nodes.first { $0.textWords == title })
        // The title's parent is the card.
        let card = tree.nodes[try #require(words.parent)]
        return card.frame.minY - visible.frame.minY
    }
}
