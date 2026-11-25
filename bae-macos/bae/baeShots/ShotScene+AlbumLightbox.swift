import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The album page's cover gallery opened on the lightbox
    /// (`mac.album.lightbox`), over the library window with the album open.
    /// The sheet is the one the album page presents: `CoverSheetView` in its
    /// `CoverGalleryFrame`, raised over the window as a modal, with the
    /// covers `mac.album.change-cover` shows.
    static let albumLightboxScenes: [ShotScene] = [
        albumLightboxScene(
            id: "mac.album.lightbox",
            opensOn: .saved,
            picture: .shown
        ),
        albumLightboxScene(
            id: "mac.album.lightbox.failed",
            opensOn: .remote(
                url: BoardRemoteCover(position: 1, type: .front).imageUrl
            ),
            picture: .noBytes
        ),
        albumLightboxScene(
            id: "mac.album.lightbox.zoom-failed",
            opensOn: .saved,
            picture: .unreadable
        ),
    ]

    /// How core's read of the picture at the lightbox's size ends.
    private enum LightboxPicture {
        /// Core reads every picture.
        case shown
        /// The catalog serves no image, so the failure has no Details.
        case noBytes
        /// Core's read of the release's cover fails, with its error behind
        /// Details.
        case unreadable
    }

    /// The widest the cover gallery asks for a picture, in pixels; the
    /// lightbox asks for more.
    nonisolated private static let galleryPixels: UInt32 = 1_600

    private static func albumLightboxScene(
        id: String,
        opensOn cover: BridgeCoverKey,
        picture: LightboxPicture
    ) -> ShotScene {
        ShotScene(
            id: id,
            size: CGSize(width: 1_200, height: 1_020)
        ) {
            let size = CGSize(width: 1_200, height: 1_020)
            let uiStore = UiStore()
            let backing = boardLibraryBacking(
                uiStore: uiStore,
                albumCount: 4,
                openAlbum: true
            )
            let gallery = boardCoverGallery()
            let images = lightboxCoverImages(picture)
            let lightboxLoad = LightboxImageLoad()
            uiStore.presentSizedModal { host in
                CoverGalleryFrame(host: host) {
                    CoverSheetView(
                        owner: .release(releaseId: "rel-board-0"),
                        opensOn: cover,
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
                switch picture {
                case .shown:
                    return lightboxLoad.phase.image != nil
                case .noBytes, .unreadable:
                    if case .failed = lightboxLoad.phase { return true }
                    return false
                }
            }
        }
    }

    /// The pictures of the cover gallery, which has the art of each cover, and
    /// of the lightbox, whose picture reads as `picture` says.
    private static func lightboxCoverImages(_ picture: LightboxPicture)
        -> ImageStore
    {
        ImageStore(
            fetchLibraryImageBytes: { image, _ in
                try BoardArtwork.data(named: image.id)
            },
            fetchReleaseImageBytes: { _, source, pixels in
                switch source {
                case .cover:
                    if case .unreadable = picture, pixels > galleryPixels {
                        throw BridgeError.Diagnostic(
                            category: .network,
                            detail: "dns error"
                        )
                    }
                    return try BoardArtwork.data(named: "Low Tide Hours")
                case .releaseFile(let id):
                    return try BoardArtwork.data(named: id)
                }
            },
            fetchRemoteImage: { image, pixels in
                if case .noBytes = picture, pixels > galleryPixels {
                    return nil
                }
                return try BoardArtwork.data(
                    named: BoardRemoteCover.artName(forUrl: image.url)
                )
            }
        )
    }
}
