import BaeKit
import Foundation
import Testing

@testable import bae

/// A browse query that records every window request and answers each one
/// with the slice of a fixed list, counting how many queries were opened.
private final class RecordingBrowse: @unchecked Sendable {
    private let lock = NSLock()
    private var requests: [[BridgeLibraryPageWindow]] = []
    private let rows: [BridgeAlbum]

    init(rows: [BridgeAlbum]) {
        self.rows = rows
    }

    var requested: [[BridgeLibraryPageWindow]] {
        lock.withLock { requests }
    }

    func query() -> LibraryBrowseQuery<BridgeAlbum> {
        let fixed = LibraryBrowseQuery<BridgeAlbum>.fixed(rows)
        return LibraryBrowseQuery(
            setWindows: { [self] windows in
                lock.withLock { requests.append(windows) }
                try fixed.setWindows(windows)
            },
            next: { try await fixed.next() },
            cancel: { await fixed.cancel() }
        )
    }
}

@Suite("LibraryBrowsePageSource")
struct LibraryBrowsePageSourceTests {
    @MainActor
    @Test("repeated album rows preserve the distinct library count")
    func distinctCount() async {
        let album = makeBridgeAlbum(id: "shared")
        let fixed = LibraryBrowseQuery<BridgeAlbum>.fixed([album, album])
        let query = LibraryBrowseQuery<BridgeAlbum>(
            setWindows: fixed.setWindows,
            next: {
                let page = try await fixed.next()
                return LibraryBrowseDelivery(
                    windows: page.windows,
                    totalCount: 2,
                    itemCount: 1
                )
            },
            cancel: fixed.cancel
        )
        var total: Int?
        let list = AlbumList(
            pageSource: LibraryAlbumPageSource(query: query),
            ingest: { _ in },
            onError: { Issue.record("Page failed: \($0)") },
            onSnapshot: { _, count in total = count }
        )
        await list.loadInitial()
        #expect(list.totalCount == 2)
        #expect(list.idAt(0) == "shared")
        #expect(list.idAt(1) == "shared")
        #expect(total == 1)
        list.cancel()
    }

    @MainActor
    @Test("artist headings and ranges arrive with the paged album rows")
    func sectionsAccompanyPages() async {
        let rows = (0..<120).map { makeBridgeAlbum(id: "album-\($0)") }
        let sections = [
            BridgeLibraryBrowseSection(
                id: "artist-a",
                title: "Artist A",
                window: .init(offset: 0, limit: 75)
            ),
            BridgeLibraryBrowseSection(
                id: "artist-b",
                title: "Artist B",
                window: .init(offset: 75, limit: 45)
            ),
        ]
        let fixed = LibraryBrowseQuery<BridgeAlbum>.fixed(rows)
        let query = LibraryBrowseQuery<BridgeAlbum>(
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
        let list = AlbumList(
            pageSource: LibraryAlbumPageSource(query: query),
            ingest: { _ in },
            onError: { Issue.record("Page failed: \($0)") }
        )
        await list.loadInitial()
        #expect(list.sections == sections)
        #expect(list.idAt(90) == nil)
        await list.withPage(containing: 90) {}
        #expect(list.idAt(90) == "album-90")
        #expect(list.sections == sections)
        list.cancel()
    }

    @MainActor
    @Test("every page of a list is read through one query's windows")
    func pagesShareOneQuery() async {
        let albums = (0..<120).map { makeBridgeAlbum(id: "album-\($0)") }
        let browse = RecordingBrowse(rows: albums)
        let store = LibraryStore()
        let list = AlbumList(
            pageSource: LibraryAlbumPageSource(query: browse.query()),
            ingest: { rows in
                for row in rows { _ = store.internAlbumSummary(row) }
            },
            onError: { _ in }
        )

        await list.loadInitial()
        await list.withPage(containing: 60) {}

        #expect(list.totalCount == 120)
        #expect(list.idAt(0) == "album-0")
        #expect(list.idAt(60) == "album-60")
        #expect(
            browse.requested.last
                == [
                    BridgeLibraryPageWindow(offset: 0, limit: 50),
                    BridgeLibraryPageWindow(offset: 50, limit: 50),
                ],
            "one request carries both pages' windows"
        )
    }

    @MainActor
    @Test("cancelling a list drops its windows from the query")
    func cancelDropsWindows() async {
        let browse = RecordingBrowse(
            rows: [makeBridgeAlbum(id: "album-0")]
        )
        let list = AlbumList(
            pageSource: LibraryAlbumPageSource(query: browse.query()),
            ingest: { _ in },
            onError: { _ in }
        )

        await list.loadInitial()
        list.cancel()

        #expect(browse.requested.last == [])
    }
}
