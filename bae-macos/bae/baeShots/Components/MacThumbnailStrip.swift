import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacThumbnailStrip` of the lightbox's four images, on the first and on
    /// the second.
    static let macThumbnailStripScenes: [ShotScene] = [
        thumbnailStrip("mac.component.thumbnail-strip.first", current: 0),
        thumbnailStrip("mac.component.thumbnail-strip.second", current: 1),
    ]

    private static func thumbnailStrip(
        _ id: String,
        current: Int
    ) -> ShotScene {
        component(id, width: 560, height: 90, images: coverStore()) {
            let items = lightboxItems()
            MacThumbnailStrip(
                items: items,
                current: items[current].id,
                centered: true,
                onSelect: { _ in },
                stroke: { _, isActive in
                    (
                        isActive ? Theme.onFill : Theme.onFillSecondary,
                        isActive ? 2 : 1
                    )
                }
            ) { item in
                Artwork(
                    content: item.image,
                    pointSize: MacThumbnailStripLayout.thumbnailSize,
                    cornerRadius: nil
                )
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Theme.backdrop)
        }
    }
}
