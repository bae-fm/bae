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
    ) async -> HostedAlbumGrid {
        let store = LibraryStore()
        let uiStore = UiStore()
        let albums = Self.albums
        let list = AlbumList(
            pageSource: LibraryAlbumPageSource(
                query: query(albums: albums, sections: sections)
            ),
            ingest: { rows in
                for row in rows { _ = store.internAlbumSummary(row) }
            },
            onError: { error in
                preconditionFailure("Hosted grid failed: \(error)")
            }
        )
        await list.loadInitial()
        if preloaded {
            for album in albums {
                _ = store.internAlbumSummary(album)
            }
            list.preloadForPreview(ids: albums.map(\.id))
        }
        uiStore.selectAlbum(openAlbumId)
        let library = Library(getAlbumIndex: { _, albumId, _ in
            albums.firstIndex { $0.id == albumId }.map(UInt64.init)
        })
        let view = AnyView(
            grid(list)
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
        let hosted = HostedAlbumGrid(
            window: window,
            host: host,
            uiStore: uiStore
        )
        await hosted.waitUntil("the grid lays out its first rows") {
            list.idAt(0) != nil
                && (hosted.scrollView?.documentView?.frame.height ?? 0) > 0
        }
        return hosted
    }

    private static func query(
        albums: [BridgeAlbum],
        sections: [BridgeLibraryBrowseSection]
    ) -> LibraryBrowseQuery<BridgeAlbum> {
        let fixed = LibraryBrowseQuery<BridgeAlbum>.fixed(albums)
        return LibraryBrowseQuery(
            setWindows: fixed.setWindows,
            next: {
                let page = try await fixed.next()
                return LibraryBrowseDelivery(
                    windows: page.windows,
                    totalCount: page.totalCount,
                    sections: sections
                )
            },
            cancel: fixed.cancel
        )
    }

    /// Lets springs, scrolls, and the page loads they set off run to the end.
    func settle() async {
        host.layoutSubtreeIfNeeded()
        try? await Task.sleep(for: .seconds(1))
        host.layoutSubtreeIfNeeded()
    }

    /// Lets the window run, laying it out, until `condition` holds: the
    /// scrolls, page loads and layouts a change sets off. Records an issue
    /// and returns false if it does not hold within ten seconds.
    @discardableResult
    func waitUntil(
        _ comment: Comment,
        sourceLocation: SourceLocation = #_sourceLocation,
        _ condition: () -> Bool
    ) async -> Bool {
        let deadline = ContinuousClock.now + .seconds(10)
        while true {
            host.layoutSubtreeIfNeeded()
            if condition() {
                return true
            }
            if ContinuousClock.now >= deadline {
                Issue.record(comment, sourceLocation: sourceLocation)
                return false
            }
            // A turn of the run loop, for SwiftUI's updates and the pages'
            // deliveries.
            try? await Task.sleep(for: .milliseconds(10))
        }
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
