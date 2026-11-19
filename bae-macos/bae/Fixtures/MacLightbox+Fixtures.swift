import BaeKit
import Foundation
import SwiftUI

@testable import bae

extension MacLightbox where Item == LightboxItem {
    /// A remote image whose bytes never arrive, which keeps the lightbox
    /// on its load.
    static func loadingPreview() -> some View {
        let image = BridgeRemoteImageSet(
            url: "https://images.example/front-original.png",
            downscaled: []
        )
        let item = LightboxItem(
            id: "front",
            label: "Front",
            image: .remote(image)
        )
        let images = ImageStore(fetchRemoteImage: { _, _ in
            try await Task.neverAnswers()
        })
        return MacLightbox(
            place: LightboxPlace(items: [item], current: item, neighbors: nil),
            load: LightboxImageLoad(),
            onSelect: { _ in },
            onDismiss: {}
        )
        .environment(images)
    }
}
