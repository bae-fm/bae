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
            systems: BridgeRatingSystems(albums: .heart, tracks: .heart),
            menu: .album(highlighted: 4)
        ),
        ratingSystemScene(
            "mac.album.rating.none",
            systems: BridgeRatingSystems(albums: .hidden, tracks: .hidden),
            menu: .album(highlighted: nil)
        ),
        ratingSystemScene(
            "mac.album.track-menu-heart",
            systems: BridgeRatingSystems(albums: .heart, tracks: .heart),
            menu: .track(highlighted: 3)
        ),
        ratingSystemScene(
            "mac.album.track-menu-none",
            systems: BridgeRatingSystems(albums: .hidden, tracks: .hidden),
            menu: .track(highlighted: nil)
        ),
    ]

    /// The menu a rating-system scene holds open, and which line the pointer
    /// is on.
    private enum OpenMenu {
        case album(highlighted: Int?)
        case track(highlighted: Int?)
    }

    /// The album page with the library's rating systems `systems`, rated as
    /// the Heart boards draw it, with `menu` open.
    private static func ratingSystemScene(
        _ id: String,
        systems: BridgeRatingSystems,
        menu: OpenMenu
    ) -> ShotScene {
        let size = CGSize(width: 1_200, height: 1_020)
        return ShotScene(id: id, size: size) {
            let uiStore = UiStore()
            let detail = ratedBoardAlbum(
                albumRating: BridgeAlbumRating(
                    mine: .five,
                    raters: [.you, .person(name: "Ana", stars: .five)]
                ),
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
                        ratingSystems: .staging(systems),
                        size: size
                    )
                    openMenu(menu, album: rated, systems: systems)
                }
            ) {
                (backing.session.albums.list?.positionCount ?? 0) > 0
            }
        }
    }

    @ViewBuilder
    private static func openMenu(
        _ menu: OpenMenu,
        album: AlbumDetail,
        systems: BridgeRatingSystems
    ) -> some View {
        switch menu {
        case .album(let highlighted):
            MacMenuInPlace(
                {
                    AlbumMenu(
                        offersSetAsPrimary: false,
                        ratingSystem: systems.albums,
                        rating: album.rating,
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
                        onRate: { _ in },
                        ratingSystem: systems.tracks,
                        playlists: .none
                    )
                },
                highlighted: highlighted
            )
            .padding(.leading, 520)
            .padding(.top, 668)
        }
    }

    /// The board's album as the person and Ana rated it, with the stars the
    /// person gave tracks 1, 4 and 7.
    private static func ratedBoardAlbum(
        albumRating: BridgeAlbumRating = BridgeAlbumRating(
            mine: .four,
            raters: [.you, .person(name: "Ana", stars: .five)]
        ),
        trackStars stars: [Int: BridgeStars] = [0: .five, 3: .four, 6: .three]
    ) -> BridgeAlbumDetail {
        var detail = PreviewData.boardAlbumDetail()
        detail.rating = albumRating
        var release = detail.releases[0]
        for (index, value) in stars {
            release.tracks[index].myStars = value
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
}
