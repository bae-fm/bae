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
        lightbox(
            "mac.component.lightbox.failed-details",
            state: .failedWithCauses
        ),
        lightbox("mac.component.lightbox.one-image", single: true),
    ]

    /// The widest a thumbnail's copy is, in pixels; the large image asks for
    /// more.
    nonisolated static let thumbnailPixels: UInt32 = 256

    /// What a lightbox shows of its image.
    private enum LightboxImageState {
        case shown, loading, failed, failedWithCauses
    }

    /// A store that gives the first image's large copy no bytes when it
    /// `failed` and never answers it while `loading`; its thumbnail and every
    /// other image have their art at once.
    private static func lightboxStore(_ state: LightboxImageState)
        -> ImageStore
    {
        let first = lightboxImages[0]
        return ImageStore(
            fetchLibraryImageBytes: { ref, pixels in
                let isLarge = (pixels ?? .max) > thumbnailPixels
                if ref.id == first, isLarge {
                    switch state {
                    case .shown:
                        return try BoardArtwork.data(named: ref.id)
                    case .loading:
                        try? await Task.sleep(for: .seconds(3_600))
                        return nil
                    case .failed:
                        return nil
                    case .failedWithCauses:
                        throw BridgeError.Diagnostic(
                            category: .network,
                            detail: "dns error"
                        )
                    }
                }
                return try BoardArtwork.data(named: ref.id)
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
                // The first image's large copy never answers, so the stage
                // cannot wait for the store to hand every image back.
                return ShotStage(
                    view,
                    isReady: {
                        guard case .loading = load.phase else { return false }
                        // Every image but the one that never answers.
                        return images.unansweredImages <= 1
                    }
                )
            case .failed, .failedWithCauses:
                return ShotStage(view, images: images) {
                    if case .failed = load.phase { return true }
                    return false
                }
            }
        }
    }
}
