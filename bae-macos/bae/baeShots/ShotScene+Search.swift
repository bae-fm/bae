import AppKit
import BaeKit
import Combine
import SwiftUI

@testable import bae

extension ShotScene {
    /// The main window with a search typed into the title bar's field, the
    /// results card hanging under the field or saying there are no matches,
    /// as the boards draw them.
    static let searchScenes: [ShotScene] = [
        ShotScene(
            id: "mac.search.results",
            size: CGSize(width: 1200, height: 760)
        ) {
            searchWindow(query: "low tide", results: lowTideResults)
        },
        ShotScene(
            id: "mac.search.results.none",
            size: CGSize(width: 1200, height: 760)
        ) {
            searchWindow(query: "zzz", results: noResults)
        },
    ]

    /// What a search for "low tide" finds: an album, an artist and two
    /// tracks.
    private static let lowTideResults = SearchResults(
        bridge: BridgeSearchResults(
            albums: [
                BridgeAlbumSearchResult(
                    id: "board-0",
                    title: "Low Tide Hours",
                    year: 1992,
                    artistLine: .named("Mara Quill"),
                    cover: nil
                )
            ],
            artists: [
                BridgeArtistSummary(
                    artistId: "artist-mara-quill",
                    name: "Mara Quill",
                    countLine: BridgeCountLabel(
                        key: "core.library.album_count",
                        count: 3
                    ),
                    image: nil
                )
            ],
            tracks: [
                searchTrack(
                    "Harbor Lights",
                    artist: "Mara Quill",
                    album: "Low Tide Hours",
                    seconds: 272
                ),
                searchTrack(
                    "Low Tide",
                    artist: "Odile Fen",
                    album: "Copper Sky",
                    seconds: 238
                ),
            ],
            composers: [],
            works: [],
            found: true,
            sectionCounts: BridgeSearchSectionCounts(
                albums: 1,
                artists: 1,
                tracks: 2,
                composers: 0,
                works: 0
            )
        ),
        query: "low tide"
    )

    /// What a search for "zzz" finds: nothing.
    private static let noResults = SearchResults(
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
        query: "zzz"
    )

    private static func searchTrack(
        _ title: String,
        artist: String,
        album: String,
        seconds: UInt8
    ) -> BridgeTrackSearchResult {
        BridgeTrackSearchResult(
            id:
                "track-\(title.lowercased().replacingOccurrences(of: " ", with: "-"))",
            title: title,
            durationClock: BridgeDurationClock(
                negative: false,
                hours: nil,
                minutes: UInt8(seconds / 60),
                seconds: UInt8(seconds % 60)
            ),
            releaseId: "release-\(album)",
            albumId: "album-\(album)",
            albumTitle: album,
            artistLine: .named(artist),
            cover: cover(album)
        )
    }

    /// The library window with `query` typed in the search field, which has
    /// the keyboard focus, and `results` in the card under it.
    private static func searchWindow(
        query: String,
        results: SearchResults
    ) -> ShotStage {
        let uiStore = UiStore()
        uiStore.searchResults = results
        uiStore.showSearchPopover = true
        let backing = boardLibraryBacking(uiStore: uiStore)
        let images = coverStore()
        let focus = SearchFocus()
        let window = ShotScenes.libraryWindow(
            uiStore: uiStore,
            backing: backing,
            playbackStore: boardPlayback(),
            images: images,
            size: CGSize(width: 1200, height: 760),
            window: AnyView(MacMainWindowContainer(searchText: query))
        )
        return ShotStage(
            window.environment(\.searchFocusRequests, focus.requests),
            images: images,
            isReady: { (backing.session.albums.list?.positionCount ?? 0) > 0 },
            // The title bar focuses the field when it hears a request, which
            // it can hear only once it is in the window, so the request goes
            // out until the field has the keyboard.
            isDrawn: { host in
                focus.requests.send()
                return host.window?.firstResponder is NSText
            }
        )
    }
}

/// The requests that the search field take the keyboard focus, as the
/// window's Search menu item sends them.
@MainActor
private final class SearchFocus {
    let requests = PassthroughSubject<Void, Never>()
}
