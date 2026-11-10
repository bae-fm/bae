import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The cover picker's remote lookup in each state that says something of
    /// itself, in its grid and in a lightbox with no image, and the picker
    /// saving a cover, with the picker at two sizes, the lightbox over an
    /// image and the picker with no release linked.
    static let coverPickerScenes: [ShotScene] = [
        ShotScene(
            id: "artwork-lightbox",
            size: CGSize(width: 1_148, height: 868)
        ) {
            let load = LightboxImageLoad()
            let images = PreviewData.artImageStore()
            return ShotStage(
                CoverPickerPreviewScene.lightbox(
                    load: load,
                    images: images
                ),
                images: images
            ) { load.ended() }
        },
        ShotScene(
            id: "cover-picker-wide",
            size: CGSize(width: 1_148, height: 868)
        ) {
            coverPicker()
        },
        ShotScene(
            id: "cover-picker-short",
            size: CGSize(width: 800, height: 520)
        ) {
            coverPicker()
        },
        ShotScene(
            id: "cover-picker-unlinked",
            size: CGSize(width: 1_148, height: 868)
        ) {
            let images = PreviewData.artImageStore()
            return ShotStage(
                CoverPickerPreviewScene.unlinked(images: images),
                images: images
            )
        },
        scene("cover-picker-fetching", .loading([])),
        scene("cover-picker-none-found", .linked([])),
        scene(
            "cover-picker-lookup-failed",
            .failed([], failure: DisplayError(line: "Artwork lookup failed"))
        ),
        scene("cover-picker-saving", .linked([]), isSaving: true),
        lightbox("artwork-lightbox-fetching", .loading([])),
        lightbox("artwork-lightbox-unlinked", .unlinked),
        lightbox("artwork-lightbox-none-found", .linked([])),
        lightbox(
            "artwork-lightbox-lookup-failed",
            .failed([], failure: DisplayError(line: "Artwork lookup failed"))
        ),
    ]

    /// The cover picker over decoded fixture artwork.
    private static func coverPicker() -> ShotStage {
        let images = PreviewData.artImageStore()
        return ShotStage(
            CoverPickerPreviewScene().environment(images),
            images: images
        )
    }

    /// A lightbox with no image to show, its lookup in `remote`.
    private static func lightbox(_ id: String, _ remote: RemoteCoverItems)
        -> ShotScene
    {
        ShotScene(id: id, size: CGSize(width: 800, height: 520)) {
            let images = ImageStore.stub()
            return ShotStage(
                CoverGalleryView(
                    remoteItems: remote,
                    releaseItems: [],
                    selectedCover: nil,
                    lightboxLoad: LightboxImageLoad(),
                    initialLayout: .lightbox,
                    onRefresh: {},
                    onFindRelease: {},
                    onSelect: { _ in },
                    onDone: {}
                )
                .environment(images),
                images: images
            )
        }
    }

    private static func scene(
        _ id: String,
        _ remote: RemoteCoverItems,
        isSaving: Bool = false
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 1_148, height: 868)) {
            let images = PreviewData.artImageStore()
            return ShotStage(
                CoverPickerPreviewScene.lookup(
                    remote,
                    isSaving: isSaving,
                    images: images
                ),
                images: images
            )
        }
    }
}
