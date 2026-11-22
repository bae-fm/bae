import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The album page with its rating: the person's four stars over "You ·
    /// Ana rated it 5", and the stars they gave three of the tracks
    /// (`mac.album.rating`), then the same page under the Heart and None
    /// systems with the album's menu open (`mac.album.rating.heart` and
    /// `.none`), and a track's menu under each (`mac.album.track-menu-heart`
    /// and `-none`).
    static let albumRatingScenes: [ShotScene] = [
        ShotScene(
            id: "mac.album.rating",
            size: CGSize(width: 1_200, height: 1_000)
        ) {
            let size = CGSize(width: 1_200, height: 1_000)
            let uiStore = UiStore()
            let backing = boardLibraryBacking(
                uiStore: uiStore,
                openAlbum: true,
                detail: ratedBoardAlbum()
            )
            return ShotStage(
                ShotScenes.libraryWindow(
                    uiStore: uiStore,
                    backing: backing,
                    playbackStore: boardPlayback(),
                    size: size
                )
            ) {
                (backing.session.albums.list?.positionCount ?? 0) > 0
            }
        },
        ratingSystemScene(
            "mac.album.rating.heart",
            system: .heart,
            menu: .album(highlighted: 7)
        ),
        ratingSystemScene(
            "mac.album.rating.none",
            system: .hidden,
            menu: .album(highlighted: nil)
        ),
        ratingSystemScene(
            "mac.album.track-menu-heart",
            system: .heart,
            menu: .track(highlighted: 4)
        ),
        ratingSystemScene(
            "mac.album.track-menu-none",
            system: .hidden,
            menu: .track(highlighted: nil)
        ),
    ]

    /// The menu a rating-system scene holds open, and which line the pointer
    /// is on.
    private enum OpenMenu {
        case album(highlighted: Int?)
        case track(highlighted: Int?)
    }

    /// The album page under the rating system `system` for its albums and its
    /// tracks, rated as the Heart boards draw it, with `menu` open.
    private static func ratingSystemScene(
        _ id: String,
        system: BridgeRatingSystem,
        menu: OpenMenu
    ) -> ShotScene {
        let size = CGSize(width: 1_200, height: 1_020)
        return ShotScene(id: id, size: size) {
            let uiStore = UiStore()
            let detail = ratedBoardAlbum(
                albumRating: system == .heart
                    ? .heart(
                        hearted: true,
                        hearters: [.you, .person(name: "Ana")]
                    ) : .hidden,
                trackRating: system == .heart
                    ? .heart(hearted: false) : .hidden,
                trackStars: [0: .five, 1: .two, 3: .four, 6: .three]
            )
            let backing = boardLibraryBacking(
                uiStore: uiStore,
                openAlbum: true,
                detail: detail
            )
            let rated = AlbumDetail(bridge: detail)
            return ShotStage(
                ZStack(alignment: .topLeading) {
                    ShotScenes.libraryWindow(
                        uiStore: uiStore,
                        backing: backing,
                        playbackStore: boardPlayback(),
                        size: size
                    )
                    openMenu(menu, album: rated)
                }
            ) {
                (backing.session.albums.list?.positionCount ?? 0) > 0
            }
        }
    }

    @ViewBuilder
    private static func openMenu(
        _ menu: OpenMenu,
        album: AlbumDetail
    ) -> some View {
        switch menu {
        case .album(let highlighted):
            MacMenuInPlace(
                {
                    AlbumMenu(
                        offersSetAsPrimary: false,
                        rating: album.rating,
                        onSetHearted: { _ in },
                        onPlay: {},
                        onShuffle: {},
                        onAddNext: {},
                        onAddToQueue: {},
                        onEdit: {},
                        onOpenStorage: {},
                        onExport: {},
                        onSaveAs: {},
                        onSetPrimaryRelease: {},
                        onDeleteRelease: {},
                        onDeleteAlbum: {}
                    )
                },
                highlighted: highlighted
            )
            .padding(.leading, 478)
            .padding(.top, 612)
        case .track(let highlighted):
            MacMenuInPlace(
                {
                    TrackMenu(
                        track: album.shownRelease.tracks[0],
                        targets: [album.shownRelease.tracks[0].id],
                        picking: nil,
                        onPlay: {},
                        onAddNext: { _ in },
                        onAddToQueue: { _ in },
                        onExportTrack: { _ in },
                        rating: album.shownRelease.tracks[0].rating,
                        onRate: { _ in },
                        onSetHearted: { _ in },
                        playlists: playlistChoices
                    )
                },
                highlighted: highlighted
            )
            .padding(.leading, 520)
            .padding(.top, 668)
        }
    }

    /// The playlists a track menu offers to add its track to: none made yet,
    /// so the menu ends in New playlist….
    private static let playlistChoices = TrackPlaylistChoices(
        listing: BridgePlaylistListing(
            listed: [],
            addTargets: [],
            total: 0,
            addTargetCount: 0,
            canCreate: true
        ),
        onAdd: { _, _ in },
        onNewPlaylist: { _ in }
    )

    /// The board's album as the person and Ana rated it, its tracks rated as
    /// `trackRating` says by default and with the stars the person gave tracks
    /// 1, 4 and 7 where `trackStars` gives them.
    private static func ratedBoardAlbum(
        albumRating: BridgeAlbumRating = .stars(
            mine: .four,
            raters: [.you, .person(name: "Ana", stars: .five)]
        ),
        trackRating: BridgeTrackRating = .stars(mine: nil),
        trackStars stars: [Int: BridgeStars] = [0: .five, 3: .four, 6: .three]
    ) -> BridgeAlbumDetail {
        var detail = PreviewData.boardAlbumDetail()
        detail.rating = albumRating
        var release = detail.releases[0]
        for index in release.tracks.indices {
            release.tracks[index].rating = trackRating
        }
        for (index, value) in stars {
            release.tracks[index].rating = rated(trackRating, value)
        }
        release.trackGroups = release.trackGroups.map { group in
            var group = group
            group.tracks = group.tracks.map { track in
                release.tracks.first { $0.id == track.id } ?? track
            }
            return group
        }
        detail.releases = [release]
        return detail
    }

    /// `base`, the track's rating when unrated, as it shows `stars`.
    private static func rated(
        _ base: BridgeTrackRating,
        _ stars: BridgeStars
    ) -> BridgeTrackRating {
        switch base {
        case .hidden: .hidden
        case .stars: .stars(mine: stars)
        case .heart: .heart(hearted: stars.number >= 3)
        }
    }
}
