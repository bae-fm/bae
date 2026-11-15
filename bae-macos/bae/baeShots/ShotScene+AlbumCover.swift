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
    /// catalog release linked: no remote section and no Refresh.
    private static func albumCoverScene(id: String, linked: Bool) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 1_200, height: 1_000)) {
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
            var content = CoverGalleryContent(boardCoverGallery())
            if !linked {
                content.remote = CoverGalleryContent.Section(
                    items: [],
                    count: 0
                )
                content.total = content.items.count
            }
            // The board's sheet: 1100 by 820, centred in the window with
            // no dimming behind it.
            let sheet = CoverGalleryView(
                content: content,
                remoteItems: linked
                    ? .linked(content.remote?.items ?? []) : .unlinked,
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
    }

    /// What core reads for the board's release: its cover, two Discogs
    /// covers and the image files in its folder.
    static func boardCoverGallery(
        remotes remoteNames: [(String, UInt32, BridgeCoverImageType)] = [
            ("front", 1, .front), ("back", 2, .back),
        ],
        files fileNames: [String] = ["cover.jpg", "back.jpg", "disc.jpg"],
        releaseId: String = "rel-board-0"
    ) -> BridgeCoverGallery {
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
                    releaseId: releaseId,
                    source: .releaseFile(fileId: name)
                ),
                label: .file(name: name)
            )
        }
        let remotes = remoteNames.map {
            remote($0.0, position: $0.1, type: $0.2)
        }
        let files = fileNames.map(file)
        return BridgeCoverGallery(
            current: BridgeGalleryCover(
                selection: nil,
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
