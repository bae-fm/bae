import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// An album grid over `albums`, hosted in a 1180 by 720 window with the stores
/// it reads, its core index lookup answered from the albums' order.
@MainActor
struct HostedAlbumGrid {
    let window: NSWindow
    let host: NSView
    let uiStore: UiStore
    /// The grid's page reads not yet answered.
    let answers: FixedAnswers

    /// Six hundred albums, `grid-0` to `grid-599`.
    static let albums: [BridgeAlbum] = (0..<600)
        .map { index in
            makeBridgeAlbum(
                id: "grid-\(index)",
                title: "Album Title \(index)",
                artistNames: "Artist Name",
                primaryReleaseId: "rel-\(index)"
            )
        }

    /// `grid` builds the view holding the grid over the list it is given.
    /// `preloaded` loads every page up front instead of each as its rows show.
    static func host(
        preloaded: Bool,
        openAlbumId: String? = nil,
        sections: [BridgeLibraryBrowseSection] = [],
        grid: (AlbumList) -> some View
    ) async throws -> HostedAlbumGrid {
        let uiStore = UiStore()
        let albums = Self.albums
        let answers = FixedAnswers()
        let list = AlbumList(
            pageSource: LibraryAlbumPageSource(
                query: query(
                    albums: albums,
                    sections: sections,
                    answers: answers
                )
            ),
            onError: { error in
                preconditionFailure("Hosted grid failed: \(error)")
            }
        )
        await list.loadInitial()
        if preloaded {
            list.preloadForPreview(rows: albums)
        }
        uiStore.selectAlbum(openAlbumId)
        let library = Library(getAlbumIndex: { _, albumId, _ in
            albums.firstIndex { $0.id == albumId }.map(UInt64.init)
        })
        let view = AnyView(
            grid(list)
                .environment(uiStore)
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
        let hosted = HostedAlbumGrid(
            window: window,
            host: host,
            uiStore: uiStore,
            answers: answers
        )
        try await hosted.settle()
        return hosted
    }

    private static func query(
        albums: [BridgeAlbum],
        sections: [BridgeLibraryBrowseSection],
        answers: FixedAnswers
    ) -> LibraryBrowseQuery<BridgeAlbum> {
        let fixed = LibraryBrowseQuery<BridgeAlbum>
            .fixed(
                albums,
                answers: answers
            )
        return LibraryBrowseQuery(
            setWindows: fixed.setWindows,
            values: PulledValues(fixed.values) { page in
                LibraryBrowseDelivery(
                    windows: page.windows,
                    totalCount: page.totalCount,
                    sections: sections
                )
            }
        )
    }

    /// Where the grid's views are and where it is scrolled to.
    private struct Geometry: Equatable {
        let frames: [CGRect]
        let offset: CGPoint
    }

    private func geometry() -> Geometry {
        host.layoutSubtreeIfNeeded()
        return Geometry(
            frames: SnapshotTestSupport.descendants(of: host).map(\.frame),
            offset: scrollView?.contentView.bounds.origin ?? .zero
        )
    }

    /// Return once every page the grid asked for has come back and its views
    /// and scroll position have stopped moving, asking again if coming to
    /// rest asked for more pages. With a `change` to make, it is made first
    /// and the grid must have moved from where it stood before it counts as
    /// at rest, since a grid that has not started to move reads as steady.
    func settle(after change: (() -> Void)? = nil) async throws {
        var before = change.map { _ in geometry() }
        change?()
        while true {
            try await Wait.until { answers.unanswered == 0 }
            try await Wait.untilSteady(changedFrom: before) { geometry() }
            if answers.unanswered == 0 { return }
            before = nil
        }
    }

    /// Return once the grid is scrolled away from its top.
    func untilScrolled() async throws {
        let clip = try #require(scrollView).contentView
        let moves = Wait.Notice(NSView.boundsDidChangeNotification, of: clip)
        try await Wait.until(notices: [moves]) { clip.bounds.origin.y > 0 }
    }

    var scrollView: NSScrollView? {
        Self.findScrollView(in: host)
    }

    private static func findScrollView(in view: NSView) -> NSScrollView? {
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
}
