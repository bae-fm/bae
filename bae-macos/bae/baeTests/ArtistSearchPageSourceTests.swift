import BaeKit
import Foundation
import Testing

@testable import bae

/// An artist search the test drives: it records every view it is pointed at,
/// answers each with the next revision, and hands `next` whatever the test
/// delivers.
private final class ArtistSearchFeed: @unchecked Sendable {
    struct View: Equatable {
        let text: String
        let windows: [BridgeLibraryPageWindow]
    }

    private let lock = NSLock()
    private var openedCount = 0
    private var views: [View] = []
    private var pending: [BridgeArtistSearchSnapshot] = []
    private var waiter:
        CheckedContinuation<BridgeArtistSearchSnapshot, any Error>?

    var opened: Int { lock.withLock { openedCount } }
    var requested: [View] { lock.withLock { views } }
    /// The revision `set_view` handed back for the latest view.
    var revision: UInt64 { lock.withLock { UInt64(views.count) } }

    func query() -> ArtistSearchQuery {
        lock.withLock { openedCount += 1 }
        return ArtistSearchQuery(
            setView: { [self] text, windows in
                lock.withLock {
                    views.append(View(text: text, windows: windows))
                    return UInt64(views.count)
                }
            },
            next: { [self] in
                try await withCheckedThrowingContinuation { continuation in
                    let ready: BridgeArtistSearchSnapshot? = lock.withLock {
                        if pending.isEmpty {
                            waiter = continuation
                            return nil
                        }
                        return pending.removeFirst()
                    }
                    if let ready { continuation.resume(returning: ready) }
                }
            },
            cancel: {}
        )
    }

    func deliver(_ snapshot: BridgeArtistSearchSnapshot) {
        let waiter:
            CheckedContinuation<BridgeArtistSearchSnapshot, any Error>? =
                lock.withLock {
                    if let waiter = self.waiter {
                        self.waiter = nil
                        return waiter
                    }
                    pending.append(snapshot)
                    return nil
                }
        waiter?.resume(returning: snapshot)
    }
}

private let firstPage = BridgeLibraryPageWindow(offset: 0, limit: 50)

private func artist(_ id: String) -> BridgeArtistSearchResult {
    BridgeArtistSearchResult(
        artist: BridgeExistingArtist(
            artistId: id,
            name: "Artist Name",
            sortName: nil,
            musicbrainzArtistId: nil,
            discogsArtistId: nil
        ),
        image: nil,
        creditNames: []
    )
}

private func snapshot(
    revision: UInt64,
    rows: [BridgeArtistSearchResult]
) -> BridgeArtistSearchSnapshot {
    BridgeArtistSearchSnapshot(
        windows: [BridgeArtistSearchWindow(window: firstPage, rows: rows)],
        totalCount: UInt64(rows.count),
        requestRevision: revision
    )
}

@Suite("ArtistSearchPageSource")
struct ArtistSearchPageSourceTests {
    @MainActor
    @Test(
        "a new text moves the one query, and values read for the old text are dropped"
    )
    func newTextMovesOneQuery() async throws {
        let feed = ArtistSearchFeed()
        let source = ArtistSearchPageSource(query: feed.query())
        let delivered = DeliveredArtists()

        source.setText("Artist")
        let first = source.subscribe(
            offset: 0,
            limit: 50,
            onValue: { rows, _ in delivered.ids.append(rows.map(\.id)) },
            onError: { _ in }
        )
        let firstText = feed.revision
        #expect(
            feed.requested.last == .init(text: "Artist", windows: [firstPage])
        )

        source.setText("Artist Name")
        first.cancel()
        let second = source.subscribe(
            offset: 0,
            limit: 50,
            onValue: { rows, _ in delivered.ids.append(rows.map(\.id)) },
            onError: { _ in }
        )
        #expect(feed.opened == 1)
        #expect(
            feed.requested.last
                == .init(text: "Artist Name", windows: [firstPage]),
            "the old list's late cancel leaves the new list's window alone"
        )

        feed.deliver(snapshot(revision: firstText, rows: [artist("artist-1")]))
        feed.deliver(
            snapshot(revision: feed.revision, rows: [artist("artist-2")])
        )
        try await Wait.until { !delivered.ids.isEmpty }

        #expect(delivered.ids == [["artist-2"]])
        _ = second
        source.close()
    }
}

@Suite("ArtistSearchResults")
struct ArtistSearchResultsTests {
    @MainActor
    @Test("a typed text's list replaces the one shown once it answers")
    func typedTextShowsItsList() async throws {
        let found = [artist("artist-1"), artist("artist-2")]
        let search = ArtistSearchResults(open: { .fixed(found) })
        #expect(search.list == nil)

        search.search("Artist")
        try await Wait.until { search.list != nil }

        #expect(search.list?.totalCount == 2)
        #expect(search.list?.row(at: 1)?.id == "artist-2")
        #expect(!search.isSearching)
        search.close()
    }
}

@MainActor
private final class DeliveredArtists {
    var ids: [[String]] = []
}
