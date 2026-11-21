import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSearchCard`, one shot for each row of its preview.
    static let macSearchCardScenes: [ShotScene] = [
        searchCard(
            "mac.component.search-card.results",
            height: 240,
            results: SearchResults(
                bridge: BridgeSearchResults(
                    albums: [
                        BridgeAlbumSearchResult(
                            id: "album-low-tide",
                            title: "Low Tide Hours",
                            year: 1992,
                            artistLine: .named("Mara Quill"),
                            cover: nil
                        )
                    ],
                    artists: [],
                    tracks: [
                        BridgeTrackSearchResult(
                            id: "track-harbor-lights",
                            title: "Harbor Lights",
                            durationClock: BridgeDurationClock(
                                negative: false,
                                hours: nil,
                                minutes: 4,
                                seconds: 32
                            ),
                            releaseId: "release-low-tide",
                            albumId: "album-low-tide",
                            albumTitle: "Low Tide Hours",
                            artistLine: .named("Mara Quill"),
                            cover: cover("Low Tide Hours")
                        )
                    ],
                    composers: [],
                    works: [],
                    found: true,
                    sectionCounts: BridgeSearchSectionCounts(
                        albums: 1,
                        artists: 0,
                        tracks: 1,
                        composers: 0,
                        works: 0
                    )
                ),
                query: "low tide"
            )
        ),
        searchCard(
            "mac.component.search-card.no-results",
            height: 270,
            results: SearchResults(
                bridge: BridgeSearchResults(
                    albums: [],
                    artists: [],
                    tracks: [],
                    composers: [],
                    works: [],
                    found: false,
                    sectionCounts: BridgeSearchSectionCounts(
                        albums: 0,
                        artists: 0,
                        tracks: 0,
                        composers: 0,
                        works: 0
                    )
                ),
                query: "harbr"
            )
        ),
    ]

    private static func searchCard(
        _ id: String,
        height: CGFloat,
        results: SearchResults
    ) -> ShotScene {
        component(id, width: 440, height: height, images: coverStore()) {
            MacSearchCard(
                results: results,
                onSelectAlbum: { _ in },
                onSelectTrack: { _ in },
                onSelectArtist: { _ in },
                onSelectComposer: { _ in },
                onSelectWork: { _ in }
            )
        }
    }
}
