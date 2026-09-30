import AppKit
import BaeKit
import SwiftUI

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
        grid: (AlbumList) -> some View
    ) async -> HostedAlbumGrid {
        let store = LibraryStore()
        let uiStore = UiStore()
        let albums = Self.albums
        let list = makeList(store: store, albums: albums)
        await list.loadInitial()
        if preloaded {
            for album in albums {
                _ = store.internAlbumSummary(album)
            }
            list.preloadForPreview(ids: albums.map(\.id))
        }
        uiStore.selectAlbum(openAlbumId)
        let library = Library(getAlbumIndex: { _, albumId in
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
        await hosted.settle()
        return hosted
    }

    /// Lets springs, scrolls, and the page loads they set off run to the end.
    func settle() async {
        host.layoutSubtreeIfNeeded()
        try? await Task.sleep(for: .seconds(1))
        host.layoutSubtreeIfNeeded()
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
