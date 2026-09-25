import BaeKit
import Foundation
import Testing

@testable import bae

/// How a closed artist field summarizes its artists: the names, and the badge
/// core's reading of the credits gives the whole field.
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
                resolutions: []
            )
        )

        #expect(summary.names == "Artist Name")
        #expect(summary.identityLabel == "Library")
    }

    @Test("a credit the library holds reads as a library artist")
    func creditInLibrarySummary() throws {
        let library = BridgeExistingArtist(
            artistId: "artist-1",
            name: "Artist Name",
            sortName: nil,
            musicbrainzArtistId: nil,
            discogsArtistId: nil
        )
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: [PreviewData.artistCredit("Artist Name")],
                resolutions: [
                    PreviewData.resolvedCredit(
                        "Artist Name",
                        .library(artist: library)
                    )
                ]
            )
        )

        #expect(summary.identityLabel == "Library")
    }

    @Test("one new artist reads as its own name and badge")
    func oneNewArtistSummary() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: [PreviewData.artistCredit("New Artist Name")],
                resolutions: [
                    PreviewData.resolvedCredit("New Artist Name", .new)
                ]
            )
        )

        #expect(summary.names == "New Artist Name")
        #expect(summary.identityLabel == "New")
    }

    @Test("a credit several library artists could be counts them")
    func ambiguousCreditSummary() throws {
        let artists = ["artist-1", "artist-2"]
            .map {
                BridgeExistingArtist(
                    artistId: $0,
                    name: "Artist Name",
                    sortName: nil,
                    musicbrainzArtistId: nil,
                    discogsArtistId: nil
                )
            }
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: [PreviewData.artistCredit("Artist Name")],
                resolutions: [
                    PreviewData.resolvedCredit(
                        "Artist Name",
                        .ambiguous(artists: artists)
                    )
                ]
            )
        )

        #expect(summary.identityLabel == "2 in library")
    }

    @Test("a credit nothing has read yet carries no badge")
    func unreadCreditSummary() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: [PreviewData.artistCredit("Artist Name")],
                resolutions: []
            )
        )

        #expect(summary.names == "Artist Name")
        #expect(summary.identityLabel == nil)
    }

    @Test("artists all already in the library carry one Library badge")
    func allLibraryArtistsSummary() throws {
        let names = ["First Artist", "Second Artist", "Third Artist"]
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: names.enumerated()
                    .map { index, name in
                        PreviewData.pickedArtist(
                            name,
                            artistId: "artist-\(index)"
                        )
                    },
                resolutions: []
            )
        )

        #expect(
            summary.names == ListFormatter.localizedString(byJoining: names)
        )
        #expect(summary.identityLabel == "Library")
    }

    @Test("artists all new to the library carry one New badge")
    func allNewArtistsSummary() throws {
        let names = ["First Artist", "Second Artist"]
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: names.map(PreviewData.artistCredit),
                resolutions: names.map { PreviewData.resolvedCredit($0, .new) }
            )
        )

        #expect(
            summary.names == ListFormatter.localizedString(byJoining: names)
        )
        #expect(summary.identityLabel == "New")
    }

    @Test("a mixed set counts the artists new to the library")
    func mixedArtistsSummaryCountsTheNewOnes() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: [
                    PreviewData.pickedArtist("First Artist", artistId: "a-1"),
                    PreviewData.artistCredit("Second Artist"),
                    PreviewData.pickedArtist("Third Artist", artistId: "a-3"),
                    PreviewData.artistCredit("Fourth Artist"),
                ],
                resolutions: [
                    PreviewData.resolvedCredit("Second Artist", .new),
                    PreviewData.resolvedCredit("Fourth Artist", .new),
                ]
            )
        )

        #expect(
            summary.names
                == ListFormatter.localizedString(
                    byJoining: [
                        "First Artist", "Second Artist", "Third Artist",
                        "Fourth Artist",
                    ]
                )
        )
        #expect(summary.identityLabel == "2 new")
    }
}

extension PreviewData {
    /// What the library holds for the name-only credit `name`.
    static func resolvedCredit(
        _ name: String,
        _ resolution: BridgeCreditResolution
    ) -> BridgeResolvedCredit {
        BridgeResolvedCredit(
            credit: BridgeArtistCredit(
                name: name,
                sortName: nil,
                musicbrainzArtistId: nil,
                discogsArtistId: nil
            ),
            resolution: resolution
        )
    }
}
