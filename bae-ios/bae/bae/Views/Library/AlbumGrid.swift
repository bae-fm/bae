import BaeKit
import SwiftUI

/// The paged album grid. Renders one card per loaded slot, resolving each
/// album's summary from the store and its cover from the primary release's
/// on-disk file. Rows that aren't loaded yet kick off their page load via
/// `.task(id:)` keyed on the list's `loadEpoch`.
struct AlbumGrid: View {
    let list: AlbumList
    let onSelect: (String) -> Void

    private let columns = [GridItem(.adaptive(minimum: 150), spacing: 12)]

    var body: some View {
        if let error = list.initialLoadError {
            LoadFailureView(line: error.line) {
                Task { await list.loadInitial() }
            }
        }
        else if list.totalCount == 0 {
            Text("No albums yet. Syncing from the cloud\u{2026}")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(32)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        else {
            ScrollView {
                LazyVGrid(columns: columns, spacing: 12) {
                    ForEach(0..<list.totalCount, id: \.self) { position in
                        AlbumCell(
                            list: list,
                            position: position,
                            onSelect: onSelect
                        )
                    }
                }
                .padding(12)
            }
        }
    }
}

/// One grid slot. Loads the page covering its position (keyed on `loadEpoch` so
/// a list swap or invalidation re-fetches), then renders the album once its id
/// resolves.
private struct AlbumCell: View {
    let list: AlbumList
    let position: Int
    let onSelect: (String) -> Void

    @Environment(LibraryStore.self)
    private var libraryStore

    var body: some View {
        Group {
            if let albumId = list.idAt(position),
                let summary = libraryStore.albumSummaries[albumId]
            {
                AlbumCard(
                    summary: summary,
                    onTap: { onSelect(albumId) }
                )
            }
            else {
                Theme.placeholder
                    .aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            }
        }
        .task(id: list.loadEpoch) {
            let offset = (position / libraryPageSize) * libraryPageSize
            await list.loadRange(offset: offset, limit: libraryPageSize)
        }
    }
}

private struct AlbumCard: View {
    let summary: AlbumSummary
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 6) {
                ImageView(imageRef: summary.cover, pointSize: 150)
                    .aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                Text(summary.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                Text(summary.artistNames)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .buttonStyle(.plain)
        // One VoiceOver element per card: announce "Title by Artist" instead of
        // the cover image + two separate text fragments. The cover is decorative
        // (its info is in the text), so it's folded into the combined label.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(
            summary.artistNames.isEmpty
                ? summary.title
                : String(
                    localized: "\(summary.title) by \(summary.artistNames)",
                    comment: "Album card VoiceOver label: title by artist"
                )
        )
    }
}

#if DEBUG
private struct AlbumGridPreview: View {
    let store = PreviewData.libraryStore()
    var body: some View {
        AlbumGrid(
            list: AlbumList.preview(albums: PreviewData.albums, store: store),
            onSelect: { _ in }
        )
        .previewStores(libraryStore: store)
    }
}

#Preview {
    AlbumGridPreview()
}
#endif
