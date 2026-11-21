import BaeKit
import Foundation
import Testing

@testable import bae

/// How a closed artist field summarizes its artists: the names, and the badge
/// core's read gives the whole field.
@Suite("Artist assignments summary")
struct ArtistAssignmentsSummaryTests {
    @Test("one library artist reads as its own name and no badge")
    func oneLibraryArtistSummary() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(
                field: BridgeArtistField(
                    standing: .library,
                    credits: [
                        BridgeStandingCredit(
                            credited: PreviewData.pickedArtist(
                                "Artist Name",
                                artistId: "artist-1"
                            ),
                            standing: .library
                        )
                    ]
                )
            )
        )

        #expect(summary.names == "Artist Name")
        #expect(summary.identityLabel == nil)
    }

    @Test("one new artist reads as its own name and badge")
    func oneNewArtistSummary() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(
                field: BridgeArtistField(
                    standing: .new,
                    credits: [
                        BridgeStandingCredit(
                            credited: PreviewData.artistCredit(
                                "New Artist Name"
                            ),
                            standing: .new
                        )
                    ]
                )
            )
        )

        #expect(summary.names == "New Artist Name")
        #expect(summary.identityLabel == "New")
    }

    @Test("a credit several library artists could be counts them")
    func ambiguousCreditSummary() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(
                field: BridgeArtistField(
                    standing: .choose(choices: 2),
                    credits: [
                        BridgeStandingCredit(
                            credited: PreviewData.artistCredit("Artist Name"),
                            standing: .choose(choices: [])
                        )
                    ]
                )
            )
        )

        #expect(summary.identityLabel == "2 in library")
    }

    @Test("several artists read as one list with one badge")
    func severalArtistsSummary() throws {
        let names = ["First Artist", "Second Artist", "Third Artist"]
        let summary = try #require(
            ArtistAssignmentsSummary(
                field: BridgeArtistField(
                    standing: .someNew(count: 2),
                    credits: [
                        BridgeStandingCredit(
                            credited: PreviewData.artistCredit(names[0]),
                            standing: .library
                        ),
                        BridgeStandingCredit(
                            credited: PreviewData.artistCredit(names[1]),
                            standing: .new
                        ),
                        BridgeStandingCredit(
                            credited: PreviewData.artistCredit(names[2]),
                            standing: .new
                        ),
                    ]
                )
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
                field: BridgeArtistField(
                    standing: .someToChoose(count: 1),
                    credits: [
                        BridgeStandingCredit(
                            credited: PreviewData.artistCredit("First Artist"),
                            standing: .choose(choices: [])
                        ),
                        BridgeStandingCredit(
                            credited: PreviewData.artistCredit("Second Artist"),
                            standing: .library
                        ),
                    ]
                )
            )
        )

        #expect(summary.identityLabel == "1 to choose")
    }

    @Test("a credit list prints each credit's printed name and join")
    func creditLineSummary() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(
                field: BridgeArtistField(
                    standing: .someNew(count: 1),
                    credits: [
                        BridgeStandingCredit(
                            credited: BridgeCredited(
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
                            standing: .library
                        ),
                        BridgeStandingCredit(
                            credited: PreviewData.artistCredit("Second Artist"),
                            standing: .new
                        ),
                    ]
                )
            )
        )

        #expect(summary.names == "First Alias feat. Second Artist")
    }

    @Test("a field naming no artist has no summary, so shows its placeholder")
    func emptyFieldHasNoSummary() {
        #expect(ArtistAssignmentsSummary(field: nil) == nil)
    }
}
