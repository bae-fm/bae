import BaeKit
import SwiftUI

@testable import bae

/// What the boards put in the library window behind the dialogs and notices
/// they draw over it: the albums the boards list, and "Low Tide Hours"
/// playing 1:42 into 5:03.
extension ShotScene {
    static let groupedAlbumsScene = ShotScene(
        id: "mac.library.albums.grouped",
        size: CGSize(width: 1200, height: 760),
        frame: .window(title: "bae", titleBar: .hidden)
    ) {
        let rows = [
            gridAlbum("Paper Lanterns", "The Slow Orchards", 1971),
            gridAlbum("Salt and Cedar", "The Slow Orchards", 1976),
            gridAlbum("Late Arrivals", "The Slow Orchards", 1974),
            gridAlbum("Small Hours", "Tomas Brell", 1965),
        ]
        .map {
            BridgeAlbumAppearance(
                album: $0,
                primaryReleaseUnplayable: false,
                displayArtistLine: nil
            )
        }
        let sections = [
            BridgeLibraryBrowseSection(
                id: "artist-slow-orchards",
                title: "The Slow Orchards",
                image: cover("The Slow Orchards"),
                firstPosition: 0,
                albumCount: 3
            ),
            BridgeLibraryBrowseSection(
                id: "artist-tomas-brell",
                title: "Tomas Brell",
                image: cover("Tomas Brell"),
                firstPosition: 3,
                albumCount: 1
            ),
        ]
        let library = Library(
            albumList: { _, _ in
                .fixed(
                    rows,
                    answers: UnansweredCount(),
                    sections: sections,
                    extra: .albumGrid(canShuffle: true, grouped: true)
                )
            },
            playlists: LibraryView.noPlaylists
        )
        let uiStore = UiStore()
        let session = LibraryBrowseSession(library: library, uiStore: uiStore)
        session.setGroupAlbumsByArtist(true)
        let images = coverStore()
        let window = ShotScenes.libraryWindow(
            uiStore: uiStore,
            backing: (library, session),
            playbackStore: boardPlayback(),
            images: images,
            size: CGSize(width: 1200, height: 760)
        )
        return ShotStage(window, images: images) {
            session.albums.list?.delivered != nil
                && session.albums.list?.sections == sections
        }
    }

    /// The albums the boards' grid lists, in their order.
    private static func boardAlbums() -> [BridgeAlbumAppearance] {
        [
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
            BridgeAlbumAppearance(
                album: BridgeAlbum(
                    id: "board-\(index)",
                    title: album.0,
                    year: Int32(album.2),
                    isCompilation: false,
                    artistLine: .named(album.1),
                    releaseIds: ["rel-board-\(index)"],
                    primaryReleaseId: "rel-board-\(index)",
                    cover: cover(album.0)
                ),
                primaryReleaseUnplayable: false,
                displayArtistLine: .named(album.1)
            )
        }

    }

    /// The board's open album, "Low Tide Hours", with the cover its tile
    /// and its page draw.
    static func boardAlbumDetail() -> BridgeAlbumDetail {
        var detail = PreviewData.boardAlbumDetail()
        let image = cover("Low Tide Hours")
        detail.album.cover = image
        detail.releases[0].cover = image
        detail.shownRelease.cover = image
        return detail
    }

    /// Where core places `albumId`'s card in the flat grid of `albums`.
    nonisolated private static func boardAlbumPlace(
        _ albumId: String,
        in albums: [BridgeAlbumAppearance]
    ) -> BridgeAlbumPlace? {
        albums.firstIndex { $0.id == albumId }
            .map { BridgeAlbumPlace(position: UInt64($0), sectionId: nil) }
    }

    /// A browse session over the first `albumCount` of `boardAlbums`, which
    /// is as many cards as the scene's board draws, with the first album open
    /// when `openAlbum` is set; `detailAnswered` learns when its detail is
    /// answered.
    @MainActor
    static func boardLibraryBacking(
        uiStore: UiStore,
        albumCount: Int? = nil,
        openAlbum: Bool = false,
        detailAnswered: AlbumDetailAnswered? = nil,
        detail: BridgeAlbumDetail = boardAlbumDetail(),
        playlists: [BridgePlaylistSummary] = [],
        playlistDetails: [String: BridgePlaylistDetail] = [:],
        history: [BridgeHistoryPlay] = [],
        historyPeople: [(id: String, name: String)] = []
    ) -> (library: Library, session: LibraryBrowseSession) {
        let allAlbums = boardAlbums()
        let albums = Array(allAlbums.prefix(albumCount ?? allAlbums.count))
        let library = Library(
            albumList: { _, _ in .fixed(albums, answers: UnansweredCount()) },
            albumPlaceReads: { _, albumId, _, _ in
                .fixed(boardAlbumPlace(albumId, in: albums))
            },
            librarySearch: {
                // A search the title bar opens for typed text; it answers
                // nothing, so the shot shows the field and no results.
                LibrarySearch(
                    setQuery: { _ in },
                    values: PulledValues(read: {
                        try await Task.sleep(for: .seconds(86_400))
                        throw StubError.notImplemented
                    })
                )
            },
            albumDetail: {
                boardAlbumDetailRead(detail, answered: detailAnswered)
            },
            releaseDetail: {
                .fixed(answers: UnansweredCount()) { id in
                    detail.releases.first { $0.id == id }
                }
            },
            playlists: { .fixed(playlistListing(playlists)) },
            playlistDetail: {
                .fixed(answers: UnansweredCount()) { playlistDetails[$0] }
            },
            historyList: { person in
                .fixed(
                    history.filter {
                        person == nil || $0.person.person == person
                    },
                    answers: UnansweredCount(),
                    extra: boardHistoryMenu(historyPeople, person: person)
                )
            },
            releaseFiles: {
                .fixed(
                    PreviewData.previewReleaseFiles,
                    answers: UnansweredCount()
                )
            }
        )
        let session = LibraryBrowseSession(library: library, uiStore: uiStore)
        session.start()
        if openAlbum {
            uiStore.selectAlbum(detail.album.id)
        }
        return (library, session)
    }

    nonisolated private static func boardAlbumDetailRead(
        _ detail: BridgeAlbumDetail,
        answered: AlbumDetailAnswered?
    ) -> KeyedDetailQuery<BridgeAlbumDetail, BridgeAlbumDetailRequest> {
        .fixed(answers: UnansweredCount()) { request in
            guard request.albumId == detail.album.id else {
                return nil
            }
            if let answered {
                Task { @MainActor in answered.answered = true }
            }
            return detail.showing(chosenReleaseId: request.chosenReleaseId)
        }
    }

    /// History's menu of whose plays as core counts it under `person`:
    /// `people` in their order, and whose plays it shows.
    /// The Playlists list of `playlists`, for a person with a profile, who
    /// can add to their own.
    nonisolated static func playlistListing(
        _ playlists: [BridgePlaylistSummary]
    ) -> BridgePlaylistListing {
        BridgePlaylistListing(
            listed: playlists,
            addTargets: playlists.filter(\.yours),
            total: UInt64(playlists.count),
            addTargetCount: UInt64(playlists.filter(\.yours).count),
            canCreate: true
        )
    }

    nonisolated static func boardHistoryMenu(
        _ people: [(id: String, name: String)],
        person: String?
    ) -> BridgeListCountExtra {
        let shown: BridgeHistoryShown =
            if let name = people.first(where: { $0.id == person })?.name {
                .person(name: name)
            }
            else {
                .everyone
            }
        return .history(
            people: people.map {
                BridgeHistoryPerson(
                    person: $0.id,
                    name: $0.name,
                    chosen: $0.id == person
                )
            },
            shown: shown
        )
    }

    /// "Low Tide Hours" by Mara Quill playing: 1:42 into 5:03, the slider at
    /// 34 percent, and 60 percent of the track held. With `removedFromLibrary`, the
    /// library no longer holds the track. `progress` is where the slider sits,
    /// as the share of the track. With `waitsOnStorage`, the track is loading
    /// and waits on a download, so the bar draws the waiting arc.
    @MainActor
    /// The id of "Low Tide Hours" in `PreviewData.boardAlbumDetail`'s track
    /// list, so the open album marks it as the track that plays.
    nonisolated static let boardTrackId = "t-d1-4"

    /// The bar with nothing playing, at the volume the boards' bar draws.
    static func idlePlayback() -> PlaybackStore {
        let store = PlaybackStore()
        store.settings = PlaybackSettings(
            volume: 0.7,
            isMuted: false,
            repeatMode: .off
        )
        return store
    }

    static func boardPlayback(
        removedFromLibrary: Bool = false,
        progress: Double = 102.0 / 303.0,
        waitsOnStorage: Bool = false
    ) -> PlaybackStore {
        let store = PlaybackStore(
            trackPositions: .trackPositions(
                holding: [
                    boardTrackId: BridgeTrackPosition(
                        positionMs: 102_000,
                        durationMs: 303_000,
                        progress: progress,
                        seeks: 0
                    )
                ]
            ),
            trackBuffered: .trackBuffered(
                holding: [
                    boardTrackId: BridgeTrackBuffered(
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
        let track = BridgeNowPlayingTrack(
            track: BridgePlayingTrack(
                trackId: boardTrackId,
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
                        coverImage: cover("Low Tide Hours")
                    )
                )
        )
        store.apply(
            waitsOnStorage
                ? .loading(
                    track: .prepared(track: track),
                    waitsOnStorage: true
                )
                : .playing(track: track)
        )
        return store
    }
}

/// Whether the fixture has answered the open album's detail read.
@MainActor
@Observable
final class AlbumDetailAnswered {
    var answered = false
}
