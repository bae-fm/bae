import BaeKit
import SwiftUI

/// The folder's images as a gallery of fixed thumbnails, each opening the
/// lightbox at itself.
///
/// A picture is read by looking at it — a row per filename says nothing about
/// what is in the file — so the images are shown rather than listed.
struct ImportMappingGallery: View {
    let images: [BridgeMappingImage]
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
        Button {
            actions.openImages(images, image.localPath)
        } label: {
            VStack(spacing: 3) {
                ImageView(
                    content: .localFile(path: image.localPath),
                    pointSize: Self.tileSize
                )
                .frame(width: Self.tileSize, height: Self.tileSize)
                .clipShape(RoundedRectangle(cornerRadius: 4))
                Text(image.name)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                // The one that leads the release says so under its own tile:
                // which image is the cover is a fact about this folder, and
                // the gallery is where it is visible.
                Text(coreString("ui.import.becomes.cover"))
                    .font(.caption2)
                    .foregroundStyle(Theme.accent)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .opacity(image.isCover ? 1 : 0)
            }
            .frame(width: Self.tileSize, alignment: .top)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
