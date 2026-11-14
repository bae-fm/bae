import BaeKit
import SwiftUI

@testable import bae

/// What the boards put in the library window behind the dialogs and notices
/// they draw over it: the albums the boards list, and "Low Tide Hours"
/// playing 1:42 into 5:03.
extension ShotScene {
    /// The albums the boards' grid lists, in their order.
    nonisolated private static let boardAlbums: [BridgeAlbum] = [
        ("Low Tide Hours", "Mara Quill", 1992),
        ("Paper Lanterns", "The Slow Orchards", 1971),
        ("Glass Harbor", "Ines Varga Trio", 1959),
        ("Copper Sky", "Odile Fen", 1972),
        ("Salt and Cedar", "The Slow Orchards", 1976),
        ("Field Recordings", "Mara Quill", 1979),
        ("Winter Kitchen", "Odile Fen", 1971),
        ("Small Hours", "Tomas Brell", 1965),
    ]
    .enumerated()
    .map { index, album in
        BridgeAlbum(
            id: "board-\(index)",
            title: album.0,
            year: Int32(album.2),
            isCompilation: false,
            artistLine: .named(album.1),
            releaseIds: ["rel-board-\(index)"],
            primaryReleaseId: "rel-board-\(index)",
            cover: nil
        )
    }

    /// A browse session over `boardAlbums`.
    @MainActor
    static func boardLibraryBacking(
        uiStore: UiStore,
        playlists: [BridgePlaylistSummary] = [],
        playlistDetails: [String: BridgePlaylistDetail] = [:]
    ) -> (library: Library, session: LibraryBrowseSession) {
        let library = Library(
            albumList: { _, _ in
                .fixed(boardAlbums, answers: UnansweredCount())
            },
            getAlbumIndex: { _, albumId, _ in
                boardAlbums.firstIndex { $0.id == albumId }
                    .map(UInt64.init)
            },
            playlists: {
                .fixed(
                    BridgePlaylistListing(
                        listed: playlists,
                        addTargets: playlists.filter(\.yours),
                        total: UInt64(playlists.count),
                        canCreate: true
                    )
                )
            },
            playlistDetail: {
                .fixed(answers: UnansweredCount()) { playlistDetails[$0] }
            }
        )
        let session = LibraryBrowseSession(library: library, uiStore: uiStore)
        session.start()
        return (library, session)
    }

    /// "Low Tide Hours" by Mara Quill playing: 1:42 into 5:03, the slider at
    /// 35 and 60 percent of the track here.
    @MainActor
    static func boardPlayback() -> PlaybackStore {
        let store = PlaybackStore(
            trackPositions: .trackPositions(
                holding: [
                    "board-track": BridgeTrackPosition(
                        positionMs: 102_000,
                        rate: 0,
                        anchoredAtNs: 0,
                        durationMs: 303_000
                    )
                ]
            ),
            trackBuffered: .trackBuffered(
                holding: [
                    "board-track": BridgeTrackBuffered(
                        ranges: [BridgeTrackSpan(start: 0, end: 0.6)]
                    )
                ]
            )
        )
        store.play(
            track: BridgeNowPlayingTrack(
                track: BridgePlayingTrack(
                    trackId: "board-track",
                    durationMs: 303_000
                ),
                display: BridgeTrackDisplay(
                    title: "Low Tide Hours",
                    artistLine: .named("Mara Quill"),
                    albumId: "board-0",
                    releaseId: "rel-board-0",
                    albumTitle: "Low Tide Hours",
                    coverImage: nil
                )
            )
        )
        return store
    }
}
