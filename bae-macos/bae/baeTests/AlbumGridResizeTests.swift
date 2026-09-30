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

    private static let albums: [BridgeAlbum] = (0..<600)
        .map { index in
            makeBridgeAlbum(
                id: "grid-\(index)",
                title: "Album Title \(index)",
                artistNames: "Artist Name",
                primaryReleaseId: "rel-\(index)"
            )
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

    private func host(
        _ stage: Stage,
        scene: Scene
    ) async -> (NSWindow, NSHostingView<AnyView>) {
        let store = LibraryStore()
        let uiStore = UiStore()
        let list = makeList(store: store, albums: Self.albums)
        await list.loadInitial()
        if scene.preloaded {
            for album in Self.albums {
                _ = store.internAlbumSummary(album)
            }
            list.preloadForPreview(ids: Self.albums.map(\.id))
        }
        uiStore.selectAlbum(scene.openAlbumId)
        let albums = Self.albums
        let library = Library(getAlbumIndex: { _, albumId in
            albums.firstIndex { $0.id == albumId }.map(UInt64.init)
        })
        let view = AnyView(
            Harness(stage: stage, list: list)
                .environment(uiStore)
                .environment(store)
                .environment(library)
                .environment(ImageStore.stub())
        )
        let frame = NSRect(x: 0, y: 0, width: 1180, height: 720)
        let host = NSHostingView(rootView: view)
        host.frame = frame
        let window = NSWindow(
            contentRect: frame,
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        window.isReleasedWhenClosed = false
        window.contentView = host
        window.orderFront(nil)
        await settle(host)
        return (window, host)
    }

    /// Lets the spring and the page loads it sets off run to the end.
    private func settle(_ host: NSView) async {
        host.layoutSubtreeIfNeeded()
        try? await Task.sleep(for: .seconds(1))
        host.layoutSubtreeIfNeeded()
    }

    private func findScrollView(in view: NSView) -> NSScrollView? {
        if let scrollView = view as? NSScrollView {
            return scrollView
        }
        for subview in view.subviews {
            if let found = findScrollView(in: subview) {
                return found
            }
        }
        return nil
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
        let (window, host) = await host(stage, scene: scene)
        defer { window.close() }
        let scrollView = try #require(findScrollView(in: host))
        let offset = { scrollView.contentView.bounds.origin.y }
        // With a detail open the grid places the rows it has not drawn by
        // estimate, so the offset is no measure; where the detail sits is.
        let place = {
            scene.openAlbumId == nil ? offset() : stage.detailTop ?? .nan
        }

        scrollView.contentView.scroll(to: NSPoint(x: 0, y: scene.scrolledTo))
        scrollView.reflectScrolledClipView(scrollView.contentView)
        await settle(host)
        let before = place()
        let content = try #require(scrollView.documentView).frame.height
        let rowPitch =
            content
            / (Double(Self.albums.count) / Self.wideColumns).rounded(.up)

        stage.showPanel = true
        await settle(host)
        let narrowed = place()
        if scene.openAlbumId == nil {
            // Fewer columns put the slot on top further down the content.
            #expect(narrowed > before)
        }

        stage.showPanel = false
        await settle(host)
        let back = place()
        // Back at five columns the slot on top is the one that was: the grid
        // moved by less than half a row, to line that slot up with the top.
        #expect(abs(back - before) <= rowPitch / 2)

        // Going back and forth again returns to the same places.
        stage.showPanel = true
        await settle(host)
        #expect(abs(place() - narrowed) < 1)
        stage.showPanel = false
        await settle(host)
        #expect(abs(place() - back) < 1)

        if scene.scrolledTo == 6450 {
            // The detail was the slot on top, and stays there.
            #expect(abs(narrowed) < 1)
            #expect(abs(back) < 1)
        }
    }
}
