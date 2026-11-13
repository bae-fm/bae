import BaeKit
import Foundation
import SwiftUI

@testable import bae

extension LightboxView where Item == LightboxItem {
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
        return Cursor(items: [item], preferring: "front")
            .map { cursor in
                LightboxView(
                    cursor: cursor,
                    load: LightboxImageLoad(),
                    onUpdate: { _ in },
                    onDismiss: {}
                )
                .environment(images)
            }
    }
}
