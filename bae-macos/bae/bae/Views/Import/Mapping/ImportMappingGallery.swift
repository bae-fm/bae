import BaeKit
import SwiftUI

/// One image of the editor's Images section: a file of the folder on disk, or
/// a file of the library release.
struct ImportImage: Identifiable {
    /// The file's id.
    let id: String
    let name: String
    let content: ImageContent
    /// What a click on the image does.
    let open: () -> Void

    /// An image of the folder, at the version the scan read; a click opens
    /// the folder's lightbox at it.
    init(_ image: BridgeMappingImage, open: @escaping (String) -> Void) {
        id = image.fileId
        name = image.name
        content = .localFile(image.file)
        self.open = { open(image.fileId) }
    }

    /// An image among the files of the library release `releaseId`; a click
    /// opens the viewer at the key core names it by.
    init(
        _ image: BridgeReleaseImage,
        releaseId: String,
        open: @escaping (BridgeCoverKey) -> Void
    ) {
        id = image.fileId
        name = image.name
        self.open = { open(image.key) }
        content = .releaseImage(
            releaseId: releaseId,
            source: image.source
        )
    }
}

/// The images as a thumbnail gallery; each opens the lightbox at itself.
struct ImportMappingGallery: View {
    let images: [ImportImage]

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
                ImportMappingGalleryTile(image: image)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// One gallery image, named on hover, outlined on hover; dragging it onto
/// the cover well makes it the cover.
struct ImportMappingGalleryTile: View {
    let image: ImportImage

    @State
    private var hovering = false

    private var tileSize: CGFloat { ImportMappingGallery.tileSize }

    var body: some View {
        Button {
            image.open()
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
