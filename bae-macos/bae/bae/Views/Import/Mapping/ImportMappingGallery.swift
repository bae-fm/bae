import BaeKit
import SwiftUI

/// The folder's images as a thumbnail gallery; each opens the lightbox at
/// itself.
struct ImportMappingGallery: View {
    let images: [BridgeMappingImage]
    /// Identifying signals extracted from the images, by source file.
    var evidence: [BridgeFileEvidence] = []
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
                    spacing: 10,
                    alignment: .top
                )
            ],
            alignment: .leading,
            spacing: 10
        ) {
            ForEach(images, id: \.fileId) { image in
                ImportMappingGalleryTile(
                    image: image,
                    images: images,
                    evidence: ImportEvidence.of(image.fileId, in: evidence),
                    onOpen: onOpen
                )
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// One gallery image over its filename, outlined on hover; dragging it onto
/// the cover well makes it the cover.
struct ImportMappingGalleryTile: View {
    let image: BridgeMappingImage
    /// Every gallery image, for the lightbox to page through.
    let images: [BridgeMappingImage]
    let evidence: [BridgeFileEvidence]
    let onOpen: ([BridgeMappingImage], String) -> Void

    @State
    private var hovering = false

    private var tileSize: CGFloat { ImportMappingGallery.tileSize }

    var body: some View {
        Button {
            onOpen(images, image.localPath)
        } label: {
            VStack(alignment: .leading, spacing: 6) {
                ImageView(
                    content: .localFile(path: image.localPath),
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
                .overlay(alignment: .bottomLeading) {
                    HStack(spacing: 3) {
                        ForEach(ImportEvidence.badges(evidence)) { badge in
                            ImportEvidenceChip(
                                signal: badge.signal,
                                onImage: true
                            )
                        }
                    }
                    .padding(4)
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
        .help(ImportEvidence.hoverText(evidence))
    }
}
