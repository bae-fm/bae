import BaeKit
import Foundation
import Testing

@testable import bae

/// How a closed artist field summarizes its artists: the names, and the badge
/// core's read gives the whole field.
@Suite("Artist assignments summary")
struct ArtistAssignmentsSummaryTests {
    @Test("one library artist reads as its own name and badge")
    func oneLibraryArtistSummary() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: [
                    PreviewData.pickedArtist(
                        "Artist Name",
                        artistId: "artist-1"
                    )
                ],
                standing: .library
            )
        )

        #expect(summary.names == "Artist Name")
        #expect(summary.identityLabel == "Library")
    }

    @Test("one new artist reads as its own name and badge")
    func oneNewArtistSummary() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: [PreviewData.artistCredit("New Artist Name")],
                standing: .new
            )
        )

        #expect(summary.names == "New Artist Name")
        #expect(summary.identityLabel == "New")
    }

    @Test("a credit several library artists could be counts them")
    func ambiguousCreditSummary() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: [PreviewData.artistCredit("Artist Name")],
                standing: .choose(choices: 2)
            )
        )

        #expect(summary.identityLabel == "2 in library")
    }

    @Test("a field core has no standing for carries no badge")
    func unreadFieldSummary() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: [PreviewData.artistCredit("Artist Name")],
                standing: nil
            )
        )

        #expect(summary.names == "Artist Name")
        #expect(summary.identityLabel == nil)
    }

    @Test("several artists read as one list with one badge")
    func severalArtistsSummary() throws {
        let names = ["First Artist", "Second Artist", "Third Artist"]
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: names.map(PreviewData.artistCredit),
                standing: .someNew(count: 2)
            )
        )

        #expect(
            summary.names == "First Artist, Second Artist, and Third Artist"
        )
        #expect(summary.identityLabel == "2 new")
    }

    @Test("artists left to choose are counted")
    func artistsToChooseSummary() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: [
                    PreviewData.artistCredit("First Artist"),
                    PreviewData.artistCredit("Second Artist"),
                ],
                standing: .someToChoose(count: 1)
            )
        )

        #expect(summary.identityLabel == "1 to choose")
    }

    @Test("a credit list prints each credit's printed name and join")
    func creditLineSummary() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: [
                    BridgeCredited(
                        artist: .picked(
                            artist: BridgeExistingArtist(
                                artistId: "a-1",
                                name: "First Artist",
                                sortName: nil,
                                musicbrainzArtistId: nil,
                                discogsArtistId: nil
                            ),
                            creditName: BridgeCreditName(
                                key: "first alias",
                                name: "First Alias"
                            )
                        ),
                        join: " feat. "
                    ),
                    PreviewData.artistCredit("Second Artist"),
                ],
                standing: .someNew(count: 1)
            )
        )

        #expect(summary.names == "First Alias feat. Second Artist")
    }

    @Test("an empty field has no summary, so it shows its placeholder")
    func emptyFieldHasNoSummary() {
        #expect(ArtistAssignmentsSummary(assignments: [], standing: nil) == nil)
    }
}
