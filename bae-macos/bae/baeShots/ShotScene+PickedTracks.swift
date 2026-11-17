import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The person has picked tracks 2, 4 and 5 of the board's album, and the
    /// menu for the picked tracks is open over them
    /// (`mac.album.play-picked-tracks`).
    static let pickedTracksScenes: [ShotScene] = [
        ShotScene(
            id: "mac.album.play-picked-tracks",
            size: CGSize(width: 1_200, height: 1_000)
        ) {
            let size = CGSize(width: 1_200, height: 1_000)
            let uiStore = UiStore()
            let backing = boardLibraryBacking(uiStore: uiStore, openAlbum: true)
            let tracks = PreviewData.boardAlbumDetail().releases[0].tracks
                .map(Track.init(from:))
            for index in [1, 3, 4] {
                uiStore.pickedTracks.set(tracks[index].id, picked: true)
            }
            return ShotStage(
                ZStack(alignment: .topLeading) {
                    ShotScenes.libraryWindow(
                        uiStore: uiStore,
                        backing: backing,
                        playbackStore: boardPlayback(),
                        size: size
                    )
                    MacMenuInPlace(
                        {
                            TrackMenu(
                                track: tracks[1],
                                targets: uiStore.pickedTracks.targets(
                                    for: tracks[1].id
                                ),
                                picking: TrackPicking(
                                    onExtend: { _ in },
                                    onPlay: { _ in },
                                    onAddNext: { _ in },
                                    onAddToQueue: { _ in }
                                ),
                                onPlay: {},
                                onAddNext: { _ in },
                                onAddToQueue: { _ in },
                                onExportTrack: { _ in },
                                onRate: { _ in },
                                playlists: TrackPlaylistChoices(
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
                            )
                        },
                        highlighted: 0
                    )
                    .padding(.leading, 520)
                    .padding(.top, 600)
                }
            ) {
                (backing.session.albums.list?.positionCount ?? 0) > 0
            }
        }
    ]
}
