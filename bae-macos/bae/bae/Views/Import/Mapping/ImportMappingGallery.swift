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

    static let tileSize: CGFloat = 96

    var body: some View {
        LazyVGrid(
            columns: [
                GridItem(
                    .adaptive(
                        minimum: Self.tileSize,
                        maximum: Self.tileSize
                    ),
                    spacing: 8,
                    alignment: .top
                )
            ],
            alignment: .leading,
            spacing: 8
        ) {
            ForEach(images, id: \.fileId, content: tile)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func tile(_ image: BridgeMappingImage) -> some View {
        let found = ImportEvidence.of(image.fileId, in: evidence)
        return Button {
            actions.openImages(images, image.localPath)
        } label: {
            VStack(spacing: 3) {
                ImageView(
                    content: .localFile(path: image.localPath),
                    pointSize: Self.tileSize
                )
                .frame(width: Self.tileSize, height: Self.tileSize)
                .clipShape(RoundedRectangle(cornerRadius: 4))
                .overlay(alignment: .bottomLeading) {
                    HStack(spacing: 3) {
                        ForEach(ImportEvidence.badges(found)) { badge in
                            ImportEvidenceChip(
                                signal: badge.signal,
                                onImage: true
                            )
                        }
                    }
                    .padding(3)
                }
                Text(image.name)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .frame(width: Self.tileSize, alignment: .top)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .draggable(image.fileId)
        .help(ImportEvidence.hoverText(found))
    }
}
