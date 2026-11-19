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

    /// Where core places `albumId`'s card in the boards' flat grid.
    nonisolated private static func boardAlbumPlace(
        _ albumId: String
    ) -> BridgeAlbumPlace? {
        boardAlbums.firstIndex { $0.id == albumId }
            .map { BridgeAlbumPlace(position: UInt64($0), sectionId: nil) }
    }

    /// A browse session over `boardAlbums`, with the first album open when
    /// `openAlbum` is set, as the album boards draw it.
    @MainActor
    static func boardLibraryBacking(
        uiStore: UiStore,
        openAlbum: Bool = false,
        detail: BridgeAlbumDetail = PreviewData.boardAlbumDetail(),
        playlists: [BridgePlaylistSummary] = [],
        playlistDetails: [String: BridgePlaylistDetail] = [:],
        coverGallery: BridgeCoverGallery = boardCoverGallery()
    ) -> (library: Library, session: LibraryBrowseSession) {
        let library = Library(
            albumList: { _, _ in
                .fixed(boardAlbums, answers: UnansweredCount())
            },
            albumPlaceReads: { _, albumId, _, _ in
                .fixed(boardAlbumPlace(albumId))
            },
            albumDetail: {
                .fixed(answers: UnansweredCount()) { request in
                    request.albumId == detail.album.id
                        ? detail.showing(
                            chosenReleaseId: request.chosenReleaseId
                        ) : nil
                }
            },
            releaseDetail: {
                .fixed(answers: UnansweredCount()) { id in
                    detail.releases.first { $0.id == id }
                }
            },
            playlists: {
                .fixed(
                    BridgePlaylistListing(
                        listed: playlists,
                        addTargets: playlists.filter(\.yours),
                        total: UInt64(playlists.count),
                        addTargetCount: UInt64(
                            playlists.filter(\.yours).count
                        ),
                        canCreate: true
                    )
                )
            },
            playlistDetail: {
                .fixed(answers: UnansweredCount()) { playlistDetails[$0] }
            },
            releaseFiles: {
                .fixed(
                    PreviewData.previewReleaseFiles,
                    answers: UnansweredCount()
                )
            },
            coverGallery: {
                .fixed(coverGallery)
            }
        )
        let session = LibraryBrowseSession(library: library, uiStore: uiStore)
        session.start()
        if openAlbum {
            uiStore.selectAlbum(detail.album.id)
        }
        return (library, session)
    }

    /// "Low Tide Hours" by Mara Quill playing: 1:42 into 5:03, the slider at
    /// 35 and 60 percent of the track here. With `removedFromLibrary`, the
    /// library no longer holds the track.
    @MainActor
    static func boardPlayback(
        removedFromLibrary: Bool = false
    ) -> PlaybackStore {
        let store = PlaybackStore(
            trackPositions: .trackPositions(
                holding: [
                    "board-track": BridgeTrackPosition(
                        positionMs: 102_000,
                        durationMs: 303_000,
                        progress: 0.35
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
        // Set the volume the boards' bar shows (70 percent, not muted) and
        // turn repeat off, so the bar shows its Mute and Repeat buttons.
        store.settings = PlaybackSettings(
            volume: 0.7,
            isMuted: false,
            repeatMode: .off
        )
        store.apply(
            .playing(
                track: BridgeNowPlayingTrack(
                    track: BridgePlayingTrack(
                        trackId: "board-track",
                        durationMs: 303_000
                    ),
                    display: removedFromLibrary
                        ? .removed
                        : .library(
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
            )
        )
        return store
    }
}
