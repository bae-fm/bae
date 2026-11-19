import BaeKit
import SwiftUI

/// The folder's images under their own ruled heading.
struct MacImportImages: View {
    let images: [BridgeMappingImage]
    let onOpen: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            MacFormHeading(title: String(localized: "Images"))
            ImportMappingGallery(images: images, onOpen: onOpen)
        }
    }
}
