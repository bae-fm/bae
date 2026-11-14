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
        let fixed = LibraryBrowseQuery<BridgeAlbum>
            .fixed(rows, answers: UnansweredCount())
        return LibraryBrowseQuery(
            setWindows: { [self] windows in
                lock.withLock { requests.append(windows) }
                try fixed.setWindows(windows)
            },
            values: fixed.values
        )
    }
}

/// How many deliveries a query has made.
private final class DeliveryCount: @unchecked Sendable {
    private let lock = NSLock()
    private var count = 0

    /// The number of the delivery being made, from one.
    func next() -> Int {
        lock.withLock {
            count += 1
            return count
        }
    }
}

@Suite("LibraryBrowsePageSource")
struct LibraryBrowsePageSourceTests {
    @MainActor
    @Test("repeated album rows preserve the distinct library count")
    func distinctCount() async {
        let album = makeBridgeAlbum(id: "shared")
        let fixed = LibraryBrowseQuery<BridgeAlbum>
            .fixed([album, album], answers: UnansweredCount())
        let query = LibraryBrowseQuery<BridgeAlbum>(
            setWindows: fixed.setWindows,
            values: PulledValues(fixed.values) { page in
                LibraryBrowseDelivery(
                    windows: page.windows,
                    totalCount: 2,
                    itemCount: 1
                )
            }
        )
        var total: Int?
        let list = OffsetPaginatedList<BridgeAlbum>(
            pageSource: LibraryBrowsePageSource(query: query),
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
    @Test("every page of a list is read through one query's windows")
    func pagesShareOneQuery() async {
        let albums = (0..<120).map { makeBridgeAlbum(id: "album-\($0)") }
        let browse = RecordingBrowse(rows: albums)
        let list = OffsetPaginatedList<BridgeAlbum>(
            pageSource: LibraryBrowsePageSource(query: browse.query()),
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
                    BridgeLibraryPageWindow(offset: 0, limit: 50, cursor: nil),
                    BridgeLibraryPageWindow(offset: 50, limit: 50, cursor: nil),
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
        let list = OffsetPaginatedList<BridgeAlbum>(
            pageSource: LibraryBrowsePageSource(query: browse.query()),
            onError: { _ in }
        )

        await list.loadInitial()
        list.cancel()

        #expect(browse.requested.last == [])
    }
}
