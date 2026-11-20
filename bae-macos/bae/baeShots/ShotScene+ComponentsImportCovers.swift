import BaeKit
import SwiftUI

@testable import bae

// The cover gallery, its tiles and its preview.

extension ShotScene {
    static let importCoverGalleryScenes: [ShotScene] =
        coverGalleryScenes + coverPreviewScenes + coverTileScenes

    // MARK: Gallery

    /// The pictures the gallery draws: the release's cover, named `cover`, and
    /// its image files, and the covers its Discogs release serves.
    static func coverGalleryImages(of cover: String = "Low Tide Hours")
        -> ImageStore
    {
        ImageStore(
            fetchLibraryImageBytes: { PreviewData.previewArtData($0.id) },
            fetchReleaseImageBytes: { _, source in
                switch source {
                case .cover: PreviewData.previewArtData(cover)
                case .releaseFile(let id): PreviewData.previewArtData(id)
                }
            },
            fetchRemoteImage: { image, _ in
                PreviewData.previewArtData(image.url)
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

    private static let coverGalleryScenes: [ShotScene] = [
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

    // MARK: Tile and preview

    /// The pictures of the tiles and the preview: every image file is the
    /// release's cover.
    private static func coverImages() -> ImageStore {
        ImageStore(
            fetchLibraryImageBytes: { PreviewData.previewArtData($0.id) },
            fetchReleaseImageBytes: { _, _ in
                PreviewData.previewArtData("cover.jpg")
            },
            fetchRemoteImage: { image, _ in
                PreviewData.previewArtData(image.url)
            }
        )
    }

    /// The gallery's entry for the image file `name` of the release.
    private static func coverEntry(_ name: String) -> BridgeGalleryCover {
        BridgeGalleryCover(
            key: .file(fileId: name),
            image: .releaseImage(
                releaseId: "rel-board-0",
                source: .releaseFile(fileId: name)
            ),
            label: .file(
                name: name,
                selection: .releaseImage(fileId: name)
            )
        )
    }

    private static func coverTileScene(
        _ id: String,
        name: String,
        selected: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: 130,
            height: 176,
            images: coverImages()
        ) {
            MacCoverTile(
                item: CoverItem(gallery: coverEntry(name)),
                selected: selected,
                onFocus: {},
                onOpenLightbox: {}
            )
            .frame(width: 130)
        }
    }

    private static let coverTileScenes: [ShotScene] = [
        coverTileScene("mac.component.cover-tile.resting", name: "cover.jpg"),
        coverTileScene(
            "mac.component.cover-tile.selected",
            name: "cover.jpg",
            selected: true
        ),
        coverTileScene(
            "mac.component.cover-tile.long-name",
            name: "cover-front-scan-600dpi.jpg"
        ),
    ]

    /// The preview of `cover.jpg`, the `position`th of six images, between
    /// the arrows that step to its neighbors.
    private static func coverPreviewScene(
        _ id: String,
        position: UInt32
    ) -> ShotScene {
        component(
            id,
            width: 360,
            height: 440,
            images: coverImages()
        ) {
            CoverPreviewStage(content: coverPreviewContent(position: position))
                .frame(width: 360)
        }
    }

    /// Six images with `cover.jpg` focused at `position`, between its
    /// neighbors.
    private static func coverPreviewContent(position: UInt32)
        -> CoverGalleryContent
    {
        var content = CoverGalleryContent.empty
        content.total = 6
        content.focus = BridgeGalleryFocus(
            cover: coverEntry("cover.jpg"),
            position: position - 1,
            neighbors: BridgeCoverNeighbors(
                previous: .file(fileId: "disc.jpg"),
                next: .file(fileId: "back.jpg")
            )
        )
        return content
    }

    private static let coverPreviewScenes: [ShotScene] = [
        coverPreviewScene("mac.component.cover-preview.first", position: 1),
        coverPreviewScene("mac.component.cover-preview.third", position: 3),
    ]
}

/// `MacCoverPreview` with the keyboard focus binding the gallery owns.
private struct CoverPreviewStage: View {
    let content: CoverGalleryContent

    @FocusState
    private var focused: Bool

    var body: some View {
        MacCoverPreview(
            content: content,
            isSaving: false,
            previewFocused: $focused,
            onOpenLightbox: {},
            onFocus: { _ in }
        )
    }
}
