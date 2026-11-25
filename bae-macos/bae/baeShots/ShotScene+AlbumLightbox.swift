import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The lightbox over the library window with the album open
    /// (`mac.album.lightbox.zoom-failed`), on the first image of four after
    /// its picture failed, with core's error behind Details.
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
            let images = lightboxStore(.failedWithCauses)
            let load = LightboxImageLoad()
            let items = lightboxItems()
            let window = ShotScenes.libraryWindow(
                uiStore: uiStore,
                backing: backing,
                playbackStore: boardPlayback(),
                images: images,
                size: size
            )
            let lightbox = MacLightbox(
                place: LightboxPlace(
                    items: items,
                    current: items[0],
                    neighbors: LightboxNeighbors(
                        previous: items[3].id,
                        next: items[1].id
                    )
                ),
                load: load,
                onSelect: { _ in },
                onDismiss: {},
                onBrowseAll: {}
            )
            return ShotStage(
                ZStack {
                    window
                    lightbox
                }
                .environment(images),
                images: images
            ) {
                guard (backing.session.albums.list?.positionCount ?? 0) > 0
                else { return false }
                if case .failed = load.phase { return true }
                return false
            }
        }
    ]
}
