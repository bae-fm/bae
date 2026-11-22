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

    /// Where core places `albumId`'s card in the flat grid of `albums`.
    nonisolated private static func boardAlbumPlace(
        _ albumId: String,
        in albums: [BridgeAlbum]
    ) -> BridgeAlbumPlace? {
        albums.firstIndex { $0.id == albumId }
            .map { BridgeAlbumPlace(position: UInt64($0), sectionId: nil) }
    }

    /// A browse session over the first `albumCount` of `boardAlbums`, which
    /// is as many cards as the scene's board draws, with the first album open
    /// when `openAlbum` is set; `detailAnswered` learns when its detail is
    /// answered.
    @MainActor
    // swiftlint:disable:next function_body_length
    static func boardLibraryBacking(
        uiStore: UiStore,
        albumCount: Int? = nil,
        openAlbum: Bool = false,
        detailAnswered: AlbumDetailAnswered? = nil,
        detail: BridgeAlbumDetail = PreviewData.boardAlbumDetail(),
        playlists: [BridgePlaylistSummary] = [],
        playlistDetails: [String: BridgePlaylistDetail] = [:],
        history: [BridgeHistoryPlay] = [],
        historyPeople: [(id: String, name: String)] = []
    ) -> (library: Library, session: LibraryBrowseSession) {
        let albums = Array(boardAlbums.prefix(albumCount ?? boardAlbums.count))
        let library = Library(
            albumList: { _, _ in
                .fixed(albums, answers: UnansweredCount())
            },
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
                .fixed(answers: UnansweredCount()) { request in
                    guard request.albumId == detail.album.id else {
                        return nil
                    }
                    if let detailAnswered {
                        Task { @MainActor in detailAnswered.answered = true }
                    }
                    return detail.showing(
                        chosenReleaseId: request.chosenReleaseId
                    )
                }
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
    /// as the share of the track.
    @MainActor
    static func boardPlayback(
        removedFromLibrary: Bool = false,
        progress: Double = 102.0 / 303.0
    ) -> PlaybackStore {
        let store = PlaybackStore(
            trackPositions: .trackPositions(
                holding: [
                    "board-track": BridgeTrackPosition(
                        positionMs: 102_000,
                        durationMs: 303_000,
                        progress: progress,
                        seeks: 0
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

/// Whether the fixture has answered the open album's detail read.
@MainActor
@Observable
final class AlbumDetailAnswered {
    var answered = false
}
