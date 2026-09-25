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
        await list.loadPage(containing: 60)

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
