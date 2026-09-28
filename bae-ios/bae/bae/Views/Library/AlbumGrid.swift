import BaeKit
import SwiftUI

/// The narrowest a grid cell gets, and the size its cover is decoded at.
private let cellWidth: CGFloat = 150

/// The paged album grid: one cell per slot, each loading its own page.
struct AlbumGrid: View {
    let list: AlbumList
    let onSelect: (String) -> Void

    private let columns = [
        GridItem(.adaptive(minimum: cellWidth), spacing: ThemeSpace.group)
    ]

    var body: some View {
        if let error = list.initialLoadError {
            ListPlaceholder {
                LoadFailureView(error: error) {
                    Task { await list.loadInitial() }
                }
            }
        }
        else if list.totalCount == 0 {
            ListPlaceholder {
                Text("No albums yet. Syncing from the cloud\u{2026}")
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(ThemeSpace.page)
            }
        }
        else {
            ScrollView {
                LazyVGrid(columns: columns, spacing: ThemeSpace.group) {
                    ForEach(0..<list.totalCount, id: \.self) { position in
                        AlbumCell(
                            list: list,
                            position: position,
                            onSelect: onSelect
                        )
                    }
                }
                .padding(ThemeSpace.group)
            }
        }
    }
}

/// One grid slot: loads its page again whenever `loadEpoch` changes, and shows
/// the album once it resolves.
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
                Rectangle().fill(Theme.placeholder)
                    .aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
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
            VStack(alignment: .leading, spacing: ThemeSpace.compact) {
                ImageView(imageRef: summary.cover, pointSize: cellWidth)
                    .aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
                Text(summary.title)
                    .themeText(.rowTitle)
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                Text(summary.artistNames)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .buttonStyle(.plain)
        // One VoiceOver element per card; the cover adds nothing to the text.
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
#Preview {
    PreviewScenes.libraryGrid()
}
#endif
