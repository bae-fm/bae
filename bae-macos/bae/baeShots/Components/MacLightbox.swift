import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacLightbox` over the cover, with the thumbnail strip and the arrows:
    /// at rest, with Browse all images, and when the image could not load.
    static let macLightboxScenes: [ShotScene] = [
        lightbox("mac.component.lightbox.resting"),
        lightbox("mac.component.lightbox.browse", browse: true),
        lightbox("mac.component.lightbox.loading", state: .loading),
        lightbox("mac.component.lightbox.failed", state: .failed),
        lightbox("mac.component.lightbox.one-image", single: true),
    ]

    /// What a lightbox shows of its image.
    private enum LightboxImageState {
        case shown, loading, failed
    }

    /// A store that gives the first image no bytes when it `failed`, and
    /// every image's art at once otherwise.
    private static func lightboxStore(_ state: LightboxImageState)
        -> ImageStore
    {
        let first = lightboxImages[0]
        return ImageStore(
            fetchLibraryImageBytes: { ref in
                if ref.id == first {
                    switch state {
                    case .shown:
                        return PreviewData.previewArtData(ref.id)
                    case .loading:
                        try? await Task.sleep(for: .seconds(3_600))
                        return nil
                    case .failed:
                        return nil
                    }
                }
                return PreviewData.previewArtData(ref.id)
            }
        )
    }

    private static func lightbox(
        _ id: String,
        state: LightboxImageState = .shown,
        browse: Bool = false,
        single: Bool = false
    ) -> ShotScene {
        ShotScene(
            id: id,
            size: CGSize(width: 800, height: 640)
        ) {
            let images = lightboxStore(state)
            let load = LightboxImageLoad()
            let items = lightboxItems()
            let view = MacLightbox(
                place: LightboxPlace(
                    items: single ? [items[0]] : items,
                    current: items[0],
                    neighbors: single
                        ? nil
                        : LightboxNeighbors(
                            previous: items[3].id,
                            next: items[1].id
                        )
                ),
                load: load,
                onSelect: { _ in },
                onDismiss: {},
                onBrowseAll: browse ? {} : nil
            )
            .environment(images)
            switch state {
            case .shown:
                return ShotStage(view, images: images) {
                    load.phase.image != nil
                }
            case .loading:
                // The first image never answers, so the stage cannot wait
                // for the store to hand every image back.
                return ShotStage(
                    view,
                    isReady: {
                        if case .loading = load.phase { return true }
                        return false
                    }
                )
            case .failed:
                return ShotStage(view, images: images) {
                    if case .failed = load.phase { return true }
                    return false
                }
            }
        }
    }
}
