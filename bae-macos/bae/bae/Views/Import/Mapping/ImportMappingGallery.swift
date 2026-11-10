import BaeKit
import SwiftUI

/// The folder's images as a thumbnail gallery; each opens the lightbox at
/// itself.
struct ImportMappingGallery: View {
    let images: [BridgeMappingImage]
    let onOpen: ([BridgeMappingImage], String) -> Void

    static let tileSize: CGFloat = 128

    var body: some View {
        LazyVGrid(
            columns: [
                GridItem(
                    .adaptive(
                        minimum: Self.tileSize,
                        maximum: Self.tileSize
                    ),
                    spacing: ThemeSpace.related,
                    alignment: .top
                )
            ],
            alignment: .leading,
            spacing: ThemeSpace.related
        ) {
            ForEach(images, id: \.fileId) { image in
                ImportMappingGalleryTile(
                    image: image,
                    images: images,
                    onOpen: onOpen
                )
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The folder's images under their own ruled heading, level with the Tracks
/// heading the table draws beneath them.
struct ImportImagesSection: View {
    let images: [BridgeMappingImage]
    let onOpen: ([BridgeMappingImage], String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            FormSectionHeader(title: String(localized: "Images"))
            ImportMappingGallery(images: images, onOpen: onOpen)
        }
    }
}

/// One gallery image over its filename, outlined on hover; dragging it onto
/// the cover well makes it the cover.
struct ImportMappingGalleryTile: View {
    let image: BridgeMappingImage
    /// Every gallery image, for the lightbox to page through.
    let images: [BridgeMappingImage]
    let onOpen: ([BridgeMappingImage], String) -> Void

    @State
    private var hovering = false

    private var tileSize: CGFloat { ImportMappingGallery.tileSize }

    var body: some View {
        Button {
            onOpen(images, image.fileId)
        } label: {
            VStack(alignment: .leading, spacing: ThemeSpace.compact) {
                ImageView(
                    content: .localFile(image.file),
                    pointSize: tileSize
                )
                .frame(width: tileSize, height: tileSize)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
                .overlay {
                    RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                        .strokeBorder(
                            Theme.accent,
                            lineWidth: hovering ? 2 : 0
                        )
                }
                Text(image.name)
                    .themeText(.mono)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .frame(width: tileSize, alignment: .topLeading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .draggable(image.fileId)
    }
}
