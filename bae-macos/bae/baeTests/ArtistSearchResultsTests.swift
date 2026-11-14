import BaeKit
import Foundation
import Testing

@testable import bae

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

        #expect(search.list?.totalCount == 2)
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

        #expect(opened.all == ["Artist"], "the text typed first is never read")
        search.close()
    }

    @MainActor
    @Test(
        "each text is a list of its own, and the later one replaces the earlier"
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
}
