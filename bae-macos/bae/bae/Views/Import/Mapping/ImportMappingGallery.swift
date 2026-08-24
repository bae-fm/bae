import BaeKit
import SwiftUI

/// The folder's images as a gallery of fixed thumbnails, each opening the
/// lightbox at itself.
///
/// A picture is read by looking at it — a row per filename says nothing about
/// what is in the file — so the images are shown rather than listed.
struct ImportMappingGallery: View {
    let images: [BridgeMappingImage]
    /// What identified the release, by the file each piece was read off. A
    /// tile whose image is one of them says so.
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
                .overlay(alignment: .topTrailing) {
                    if let found {
                        ImportEvidenceMark(signal: found.signal)
                    }
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
        .help(found.map(ImportEvidence.hoverText) ?? "")
    }
}
