import BaeKit
import SwiftUI

/// The artists browse list: one row per loaded slot, paged from the store.
struct ArtistListView: View {
    let list: ArtistList
    let onSelect: (String) -> Void

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
                Text("No artists")
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                    .padding(ThemeSpace.page)
            }
        }
        else {
            List {
                ForEach(0..<list.totalCount, id: \.self) { position in
                    ArtistRowSlot(
                        list: list,
                        position: position,
                        onSelect: onSelect
                    )
                }
            }
            .listStyle(.plain)
        }
    }
}

private struct ArtistRowSlot: View {
    let list: ArtistList
    let position: Int
    let onSelect: (String) -> Void

    @Environment(LibraryStore.self)
    private var libraryStore

    var body: some View {
        Group {
            if let id = list.idAt(position),
                let summary = libraryStore.artistSummaries[id]
            {
                Button {
                    onSelect(id)
                } label: {
                    ArtistSummaryRow(summary: summary)
                }
                .buttonStyle(.plain)
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .task(id: list.loadEpoch) {
            let offset = (position / libraryPageSize) * libraryPageSize
            await list.loadRange(offset: offset, limit: libraryPageSize)
        }
    }
}

struct ArtistSummaryRow: View {
    let summary: BridgeArtistSummary

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            ImageView(imageRef: summary.image, pointSize: ThemeSize.rowArtwork)
                .frame(width: ThemeSize.rowArtwork, height: ThemeSize.rowArtwork)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            TwoLineRow(
                title: summary.name,
                subtitle: String(localized: "\(summary.albumCount) albums")
            )
            Spacer()
        }
        .padding(.vertical, ThemeSpace.inline)
    }
}

#if DEBUG
#Preview {
    List {
        ArtistSummaryRow(summary: PreviewData.artistSummary)
    }
    .previewStores()
}
#endif
