import BaeKit
import SwiftUI
import Testing

@testable import bae

extension ShotScene {
    /// The person has picked tracks 2, 4 and 5 of the board's album, and the
    /// menu for the picked tracks is open over them
    /// (`mac.album.play-picked-tracks`).
    static let pickedTracksScenes: [ShotScene] = [
        ShotScene(
            id: "mac.album.play-picked-tracks",
            size: CGSize(width: 1_200, height: 1_020),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            let size = CGSize(width: 1_200, height: 1_020)
            let uiStore = UiStore()
            let backing = boardLibraryBacking(
                uiStore: uiStore,
                albumCount: 4,
                openAlbum: true
            )
            let images = coverGalleryImages()
            let tracks = PreviewData.boardAlbumDetail().releases[0].tracks
                .map(Track.init(from:))
            let picked = [1, 3, 4].map { tracks[$0].id }
            var didPick = false
            return ShotStage(
                ZStack(alignment: .topLeading) {
                    ShotScenes.libraryWindow(
                        uiStore: uiStore,
                        backing: backing,
                        playbackStore: boardPlayback(),
                        images: images,
                        size: size
                    )
                    PickedTrackMenu(uiStore: uiStore, track: tracks[1])
                        .padding(.leading, 520)
                        .padding(.top, 600)
                },
                images: images,
                isReady: { backing.session.albums.list?.delivered != nil },
                isDrawn: { host in
                    // The album clears its picks when the first release
                    // appears. Pick the rows after the view draws that release.
                    if didPick { return true }
                    guard let window = host.window else { return false }
                    do {
                        let list = try ShotElementList.read(
                            board: "the menu of the picked tracks",
                            window: window,
                            size: size,
                            scale: ShotRenderer.scale
                        )
                        guard
                            list.elements.contains(where: {
                                $0.role == "popup"
                                    && $0.label == "Album options"
                            })
                        else { return false }
                        for id in picked {
                            uiStore.pickedTracks.set(id, picked: true)
                        }
                        didPick = true
                        return false
                    }
                    catch {
                        Issue.record(error)
                        return true
                    }
                }
            )
        }
    ]
}

/// The track menu over the rows the person picked. It reads the picks in its
/// own body, so it draws again once the scene has picked the rows.
private struct PickedTrackMenu: View {
    let uiStore: UiStore
    let track: Track

    var body: some View {
        MacMenuInPlace(
            {
                TrackMenu(
                    track: track,
                    targets: uiStore.pickedTracks.targets(for: track.id),
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
                    rating: .stars(mine: nil),
                    onRate: { _ in },
                    onSetHearted: { _ in },
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
    }
}
