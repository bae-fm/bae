import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The album page's cover gallery opened on the lightbox
    /// (`mac.album.lightbox.zoom-failed`), over the library window with the
    /// album open, after the release's cover failed with core's error behind
    /// Details. The sheet is the one the album page presents: `CoverSheetView`
    /// in its `CoverGalleryFrame`, raised over the window as a modal, with
    /// the release's cover and three image files.
    static let albumLightboxScenes: [ShotScene] = [
        ShotScene(
            id: "mac.album.lightbox.zoom-failed",
            size: CGSize(width: 1_200, height: 1_020)
        ) {
            let size = CGSize(width: 1_200, height: 1_020)
            let uiStore = UiStore()
            let backing = boardLibraryBacking(
                uiStore: uiStore,
                albumCount: 4,
                openAlbum: true
            )
            let gallery = boardCoverGallery(
                remotes: [],
                files: ["back.jpg", "disc.jpg", "booklet.jpg"],
                linked: false
            )
            let images = failedCoverImages()
            let lightboxLoad = LightboxImageLoad()
            uiStore.presentSizedModal { host in
                CoverGalleryFrame(host: host) {
                    CoverSheetView(
                        owner: .release(releaseId: "rel-board-0"),
                        opensOn: .saved,
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
                    images: images,
                    size: size
                ),
                images: images
            ) {
                guard (backing.session.albums.list?.positionCount ?? 0) > 0
                else { return false }
                if case .failed = lightboxLoad.phase { return true }
                return false
            }
        }
    ]

    /// The pictures of the cover gallery, whose release cover has no large
    /// copy: core fails reading it, and its small copies have their art.
    private static func failedCoverImages() -> ImageStore {
        ImageStore(
            fetchLibraryImageBytes: { image, _ in
                try BoardArtwork.data(named: image.id)
            },
            fetchReleaseImageBytes: { _, source, pixels in
                switch source {
                case .cover:
                    if (pixels ?? .max) > thumbnailPixels {
                        throw BridgeError.Diagnostic(
                            category: .network,
                            detail: "dns error"
                        )
                    }
                    return try BoardArtwork.data(named: "Low Tide Hours")
                case .releaseFile(let id):
                    return try BoardArtwork.data(named: id)
                }
            }
        )
    }
}
