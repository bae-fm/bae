import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macCoverGalleryScenes: [ShotScene] = [
        component(
            "mac.component.cover-gallery.resting",
            width: 900,
            height: 520,
            images: coverGalleryImages()
        ) {
            importStores(
                coverGallery(boardCoverGallery()) {
                    .linked($0.remote?.items ?? [])
                }
            )
        },
        component(
            "mac.component.cover-gallery.no-linked-release",
            width: 900,
            height: 520,
            images: coverGalleryImages()
        ) {
            importStores(
                coverGallery(
                    boardCoverGallery(remotes: [], linked: false)
                ) { _ in .unlinked }
            )
        },
    ]

    /// The pictures the gallery draws: the release's cover, named `cover`, and
    /// its image files, and the covers its Discogs release serves.
    static func coverGalleryImages(of cover: String = "Low Tide Hours")
        -> ImageStore
    {
        ImageStore(
            fetchLibraryImageBytes: { try BoardArtwork.data(named: $0.id) },
            fetchReleaseImageBytes: { _, source in
                switch source {
                case .cover: try BoardArtwork.data(named: cover)
                case .releaseFile(let id): try BoardArtwork.data(named: id)
                }
            },
            fetchRemoteImage: { image, _ in
                try BoardArtwork.data(named: image.url)
            }
        )
    }

    private static func coverGallery(
        _ gallery: BridgeCoverGallery,
        remoteItems: (CoverGalleryContent) -> RemoteCoverItems
    ) -> some View {
        let content = CoverGalleryContent(
            gallery.focused(on: .file(fileId: "cover.jpg"))
        )
        return MacCoverGallery(
            content: content,
            remoteItems: remoteItems(content),
            filter: .constant(.all),
            focusedCover: .constant(nil),
            selectedCover: nil,
            lightboxLoad: LightboxImageLoad(),
            onRefresh: {},
            onFindRelease: {},
            onSelect: { _ in },
            onDone: {}
        )
        .frame(width: 900, height: 520)
    }
}
