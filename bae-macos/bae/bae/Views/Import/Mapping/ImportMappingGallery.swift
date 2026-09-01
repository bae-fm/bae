import BaeKit
import SwiftUI

/// The folder's images as a gallery of fixed thumbnails, each opening the
/// lightbox at itself.
///
/// A picture is read by looking at it — a row per filename says nothing about
/// what is in the file — so the images are shown rather than listed.
struct ImportMappingGallery: View {
    let images: [BridgeMappingImage]
    /// Extracted identifying signals by their source file. A tile whose image
    /// supplied one says so, independently of the selected pressing.
    var evidence: [BridgeFileEvidence] = []
    let actions: ImportMappingActions

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
                    actions: actions
                )
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// One image of the gallery: the picture, its filename under it, and an
/// accent outline while the pointer says it can be opened. Dragging it onto
/// the cover well makes it the cover.
struct ImportMappingGalleryTile: View {
    let image: BridgeMappingImage
    /// Every image of the gallery — what the lightbox pages through from this
    /// one.
    let images: [BridgeMappingImage]
    let evidence: [BridgeFileEvidence]
    let actions: ImportMappingActions

    @State
    private var hovering = false

    private var tileSize: CGFloat { ImportMappingGallery.tileSize }

    var body: some View {
        Button {
            actions.openImages(images, image.localPath)
        } label: {
            VStack(alignment: .leading, spacing: 6) {
                ImageView(
                    content: .localFile(path: image.localPath),
                    pointSize: tileSize
                )
                .frame(width: tileSize, height: tileSize)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .overlay {
                    RoundedRectangle(cornerRadius: 6)
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
                    .font(.system(size: 10.5, design: .monospaced))
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
