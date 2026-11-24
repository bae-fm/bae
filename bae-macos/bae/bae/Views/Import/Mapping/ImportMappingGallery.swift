import BaeKit
import SwiftUI

/// One image of the editor's Images section: a file of the folder on disk, or
/// a file of the library release.
struct ImportImage: Identifiable, Equatable {
    /// The file's id, which a click hands to the lightbox.
    let id: String
    let name: String
    let content: ImageContent

    /// An image of the folder, at the version the scan read.
    init(_ image: BridgeMappingImage) {
        id = image.fileId
        name = image.name
        content = .localFile(image.file)
    }

    /// An image among the files of the library release `releaseId`.
    init(_ image: BridgeReleaseImage, releaseId: String) {
        id = image.fileId
        name = image.name
        content = .releaseImage(
            releaseId: releaseId,
            source: .releaseFile(fileId: image.fileId)
        )
    }
}

/// The images as a thumbnail gallery; each opens the lightbox at itself.
struct ImportMappingGallery: View {
    let images: [ImportImage]
    let onOpen: (String) -> Void

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
            ForEach(images) { image in
                ImportMappingGalleryTile(image: image, onOpen: onOpen)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// One gallery image, named on hover, outlined on hover; dragging it onto
/// the cover well makes it the cover.
struct ImportMappingGalleryTile: View {
    let image: ImportImage
    let onOpen: (String) -> Void

    @State
    private var hovering = false

    private var tileSize: CGFloat { ImportMappingGallery.tileSize }

    var body: some View {
        Button {
            onOpen(image.id)
        } label: {
            Artwork(
                content: image.content,
                pointSize: tileSize,
                cornerRadius: nil
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
            .frame(width: tileSize, alignment: .topLeading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.accentPlain)
        .help(image.name)
        .accessibilityLabel(Text(verbatim: image.name))
        .onHover { hovering = $0 }
        .draggable(image.id)
    }
}
