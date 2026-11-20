import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The album page's cover sheet (`mac.album.change-cover`), over the
    /// library window with the album open: the release's cover, two covers
    /// from Discogs and three image files of the release.
    static let albumCoverScenes: [ShotScene] = [
        albumCoverScene(id: "mac.album.change-cover", linked: true),
        albumCoverScene(
            id: "mac.album.change-cover.no-linked-release",
            linked: false
        ),
    ]

    /// `mac.album.change-cover.no-linked-release` draws the release with no
    /// catalog release linked: no remote section and no Refresh. The sheet
    /// is the one the album page presents: `CoverSheetView` in its
    /// `CoverGalleryFrame`, raised over the window as a modal.
    private static func albumCoverScene(id: String, linked: Bool) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 1_200, height: 1_000)) {
            let size = CGSize(width: 1_200, height: 1_000)
            let uiStore = UiStore()
            let backing = boardLibraryBacking(uiStore: uiStore, openAlbum: true)
            let gallery =
                linked
                ? boardCoverGallery()
                : boardCoverGallery(remotes: [], linked: false)
            let images = coverGalleryImages(of: "Low Tide Hours")
            let lightboxLoad = LightboxImageLoad()
            uiStore.presentModal {
                CoverGalleryFrame {
                    CoverSheetView(
                        owner: .release(releaseId: "rel-board-0"),
                        lightboxLoad: lightboxLoad,
                        onFindRelease: {},
                        refreshCovers: {},
                        onSelect: { _ in },
                        onDone: {}
                    )
                }
                .environment(images)
                .environment(Importer(coverGallery: { .fixed(gallery) }))
            }
            return ShotStage(
                ShotScenes.libraryWindow(
                    uiStore: uiStore,
                    backing: backing,
                    playbackStore: boardPlayback(),
                    size: size
                ),
                images: images
            ) {
                (backing.session.albums.list?.positionCount ?? 0) > 0
            }
        }
    }

    /// A cover the board's release has on Discogs: its image's name, its
    /// place in the release's Discogs gallery and the side it shows.
    struct BoardRemoteCover {
        let name: String
        let position: UInt32
        let type: BridgeCoverImageType
    }

    /// What core reads for the board's release: its cover, two Discogs
    /// covers and the image files in its folder, focused on its cover. With
    /// `linked` false, core reads it as a release that names no catalog
    /// release.
    static func boardCoverGallery(
        remotes remoteCovers: [BoardRemoteCover] = [
            BoardRemoteCover(name: "front", position: 1, type: .front),
            BoardRemoteCover(name: "back", position: 2, type: .back),
        ],
        files fileNames: [String] = ["cover.jpg", "back.jpg", "disc.jpg"],
        releaseId: String = "rel-board-0",
        linked: Bool = true
    ) -> BridgeCoverGallery {
        let remotes = remoteCovers.map(boardGalleryCover)
        let files = fileNames.map {
            boardGalleryCover(file: $0, releaseId: releaseId)
        }
        return BridgeCoverGallery(
            current: BridgeGalleryCover(
                key: .saved,
                image: .releaseImage(
                    releaseId: releaseId,
                    source: .cover(
                        image: BridgeImageRef(
                            id: releaseId,
                            version: "1",
                            imageType: .cover
                        )
                    )
                ),
                label: .current
            ),
            fetch: linked ? .linked : .unlinked,
            remote: BridgeCoverSection(
                items: remotes,
                count: UInt32(remotes.count)
            ),
            files: BridgeCoverSection(
                items: files,
                count: UInt32(files.count)
            ),
            total: UInt32(1 + remotes.count + files.count),
            focus: nil
        )
        .focused(on: nil)
    }

    /// The gallery entry core reads for `cover`, a Discogs cover.
    private static func boardGalleryCover(
        _ cover: BoardRemoteCover
    ) -> BridgeGalleryCover {
        let image = BridgeRemoteImageSet(
            url: "https://images.example/\(cover.name).png",
            downscaled: []
        )
        return BridgeGalleryCover(
            key: .remote(url: image.url),
            image: .remote(image: image),
            label: .remote(
                sourceName: "Discogs",
                label: .gallery(
                    reference: nil,
                    position: cover.position,
                    types: [cover.type],
                    comment: ""
                ),
                selection: .remoteCover(
                    selection: BridgeRemoteCoverSelection(
                        image: image,
                        source: .discogs
                    )
                )
            )
        )
    }

    /// The gallery entry core reads for the image file `name` in the folder
    /// of the release `releaseId`.
    private static func boardGalleryCover(
        file name: String,
        releaseId: String
    ) -> BridgeGalleryCover {
        BridgeGalleryCover(
            key: .file(fileId: name),
            image: .releaseImage(
                releaseId: releaseId,
                source: .releaseFile(fileId: name)
            ),
            label: .file(
                name: name,
                selection: .releaseImage(fileId: name)
            )
        )
    }
}
