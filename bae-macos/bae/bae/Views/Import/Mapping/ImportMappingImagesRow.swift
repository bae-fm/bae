import BaeKit
import SwiftUI

/// The folder's images, as the one row of the mapping table they share: a
/// gallery of thumbnails, each opening the lightbox at itself.
///
/// A picture is read by looking at it — a row per filename says nothing about
/// what is in the file — so the images are shown rather than listed, and what
/// becomes of them is stated once for the group.
struct ImportMappingImagesRow: View {
    let images: [BridgeMappingImage]
    /// The widths the table resolved for this pane, so the gallery ends where
    /// every other row's source cell does.
    let columns: ImportMappingColumns
    let actions: ImportMappingActions

    private static let tileSize: CGFloat = 96

    var body: some View {
        HStack(alignment: .top, spacing: ImportMappingColumns.spacing) {
            LazyVGrid(
                columns: [
                    GridItem(.adaptive(minimum: Self.tileSize), spacing: 8)
                ],
                alignment: .leading,
                spacing: 8
            ) {
                ForEach(images, id: \.fileId, content: tile)
            }
            .sourceColumn(columns)
            Spacer().frame(width: columns.role)
            Spacer().frame(width: ImportMappingColumns.position)
            Text(coreString("ui.import.becomes.kept"))
                .font(.system(size: 12))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .frame(width: columns.title, alignment: .leading)
            Spacer().frame(width: columns.artist)
            Spacer().frame(width: ImportMappingColumns.trailingColumns)
        }
    }

    private func tile(_ image: BridgeMappingImage) -> some View {
        Button {
            actions.openImages(images, image.localPath)
        } label: {
            VStack(spacing: 3) {
                Color.clear
                    .aspectRatio(1, contentMode: .fit)
                    .overlay {
                        ImageView(
                            content: .localFile(path: image.localPath),
                            pointSize: Self.tileSize
                        )
                    }
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
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
