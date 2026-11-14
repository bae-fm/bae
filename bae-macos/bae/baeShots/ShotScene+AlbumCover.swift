import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The album page's cover sheet (`mac.album.change-cover`), over the
    /// library window with the album open: the release's cover, two covers
    /// from Discogs and three image files of the release.
    static let albumCoverScenes: [ShotScene] = [
        ShotScene(
            id: "mac.album.change-cover",
            size: CGSize(width: 1_200, height: 1_000)
        ) {
            let size = CGSize(width: 1_200, height: 1_000)
            let uiStore = UiStore()
            let backing = boardLibraryBacking(uiStore: uiStore, openAlbum: true)
            let images = ImageStore(
                fetchLibraryImageBytes: { PreviewData.previewArtData($0.id) },
                fetchReleaseImageBytes: { _, source in
                    switch source {
                    case .cover: PreviewData.previewArtData("Low Tide Hours")
                    case .releaseFile(let id): PreviewData.previewArtData(id)
                    }
                },
                fetchRemoteImage: { image, _ in
                    PreviewData.previewArtData(image.url)
                }
            )
            let content = CoverGalleryContent(boardCoverGallery)
            // The board's sheet: 1100 by 820, centred in the window with
            // no dimming behind it.
            let sheet = CoverGalleryView(
                content: content,
                remoteItems: .linked(content.remote?.items ?? []),
                filter: .constant(.all),
                selectedCover: nil,
                lightboxLoad: LightboxImageLoad(),
                onRefresh: {},
                onFindRelease: {},
                onSelect: { _ in },
                onDone: {}
            )
            .frame(width: 1_100, height: 820)
            .background(Color(nsColor: .windowBackgroundColor))
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.card))
            .shadow(color: .black.opacity(0.3), radius: 20, y: 10)
            .environment(images)
            return ShotStage(
                ZStack {
                    ShotScenes.libraryWindow(
                        uiStore: uiStore,
                        backing: backing,
                        playbackStore: boardPlayback(),
                        size: size
                    )
                    sheet
                },
                images: images
            ) {
                (backing.session.albums.list?.totalCount ?? 0) > 0
            }
        }
    ]

    /// What core reads for the board's release: its cover, two Discogs
    /// covers and the image files in its folder.
    private static var boardCoverGallery: BridgeCoverGallery {
        func remote(
            _ name: String,
            position: UInt32,
            type: BridgeCoverImageType
        )
            -> BridgeGalleryCover
        {
            let image = BridgeRemoteImageSet(
                url: "https://images.example/\(name).png",
                downscaled: []
            )
            return BridgeGalleryCover(
                selection: .remoteCover(
                    selection: BridgeRemoteCoverSelection(
                        image: image,
                        source: .discogs
                    )
                ),
                image: .remote(image: image),
                label: .remote(
                    sourceName: "Discogs",
                    label: .gallery(
                        reference: nil,
                        position: position,
                        types: [type],
                        comment: ""
                    )
                )
            )
        }
        func file(_ name: String) -> BridgeGalleryCover {
            BridgeGalleryCover(
                selection: .releaseImage(fileId: name),
                image: .releaseImage(
                    releaseId: "rel-board-0",
                    source: .releaseFile(fileId: name)
                ),
                label: .file(name: name)
            )
        }
        let remotes = [
            remote("front", position: 1, type: .front),
            remote("back", position: 2, type: .back),
        ]
        let files = ["cover.jpg", "back.jpg", "disc.jpg"].map(file)
        return BridgeCoverGallery(
            current: BridgeGalleryCover(
                selection: nil,
                image: .releaseImage(
                    releaseId: "rel-board-0",
                    source: .cover(
                        image: BridgeImageRef(
                            id: "rel-board-0",
                            version: "1",
                            imageType: .cover
                        )
                    )
                ),
                label: .current
            ),
            fetch: .linked,
            sections: [
                BridgeCoverSection(
                    source: .remote,
                    items: remotes,
                    count: UInt32(remotes.count)
                ),
                BridgeCoverSection(
                    source: .releaseFiles,
                    items: files,
                    count: UInt32(files.count)
                ),
            ],
            total: UInt32(1 + remotes.count + files.count)
        )
    }
}
