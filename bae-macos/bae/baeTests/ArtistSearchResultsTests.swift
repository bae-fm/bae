import BaeKit
import Foundation
import Testing

@testable import bae

private struct ReadFailed: Error {}

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

/// The texts an artist picker's search opened lists for, in order.
private final class OpenedTexts: @unchecked Sendable {
    private let lock = NSLock()
    private var texts: [String] = []

    var all: [String] { lock.withLock { texts } }

    func opener(
        _ found: @escaping @Sendable (String) -> [BridgeArtistSearchResult]
    ) -> ArtistSearchOpener {
        { [self] text in
            lock.withLock { texts.append(text) }
            return .fixed(found(text), answers: UnansweredCount())
        }
    }
}

@Suite("ArtistSearchResults")
struct ArtistSearchResultsTests {
    @MainActor
    @Test("a typed text's list replaces the one shown once it answers")
    func typedTextShowsItsList() async throws {
        let found = [artist("artist-1"), artist("artist-2")]
        let clock = ManualClock()
        let search = ArtistSearchResults(
            open: { _ in .fixed(found, answers: UnansweredCount()) },
            clock: clock
        )
        #expect(search.list == nil)

        search.search("Artist")
        await clock.untilSleeps(begun: 1)
        clock.advance(by: .milliseconds(200))
        try await Wait.until { search.list != nil }

        #expect(search.list?.count?.rows == 2)
        #expect(search.list?.row(at: 1)?.id == "artist-2")
        search.close()
    }

    @MainActor
    @Test("typing again inside the pause opens a list for the last text only")
    func typingAgainOpensOnce() async throws {
        let opened = OpenedTexts()
        let clock = ManualClock()
        let search = ArtistSearchResults(
            open: opened.opener { _ in [artist("artist-1")] },
            clock: clock
        )

        search.search("Art")
        await clock.untilSleeps(begun: 1)
        search.search("Artist")
        await clock.untilSleeps(begun: 2)
        clock.advance(by: .milliseconds(200))
        try await Wait.until { search.list != nil }

        #expect(
            opened.all == ["Artist"],
            "the search never opens a list for the text typed first"
        )
        search.close()
    }

    @MainActor
    @Test(
        "the search opens a list for each text, and the later list replaces the earlier one"
    )
    func eachTextIsItsOwnList() async throws {
        let opened = OpenedTexts()
        let clock = ManualClock()
        let search = ArtistSearchResults(
            open: opened.opener { text in
                [artist(text == "First" ? "artist-1" : "artist-2")]
            },
            clock: clock
        )

        search.search("First")
        await clock.untilSleeps(begun: 1)
        clock.advance(by: .milliseconds(200))
        try await Wait.until { search.list?.row(at: 0)?.id == "artist-1" }

        search.search("Second")
        await clock.untilSleeps(begun: 2)
        clock.advance(by: .milliseconds(200))
        try await Wait.until { search.list?.row(at: 0)?.id == "artist-2" }

        #expect(opened.all == ["First", "Second"])
        search.close()
    }

    @MainActor
    @Test("a text whose read fails shows the failure, not the list before it")
    func failedTextShowsItsFailure() async throws {
        let clock = ManualClock()
        let search = ArtistSearchResults(
            open: { text in
                text == "First"
                    ? .fixed([artist("artist-1")], answers: UnansweredCount())
                    : .failing(ReadFailed(), answers: UnansweredCount())
            },
            clock: clock
        )

        search.search("First")
        await clock.untilSleeps(begun: 1)
        clock.advance(by: .milliseconds(200))
        try await Wait.until { search.list?.row(at: 0)?.id == "artist-1" }

        search.search("Second")
        await clock.untilSleeps(begun: 2)
        clock.advance(by: .milliseconds(200))
        try await Wait.until { search.error != nil }

        #expect(search.list == nil, "the first text's list is gone")
        search.close()
    }
}
