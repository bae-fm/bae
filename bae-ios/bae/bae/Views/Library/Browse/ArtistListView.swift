import BaeKit
import SwiftUI

/// The artists browse list: one row per loaded slot, paged from the store.
struct ArtistListView: View {
    let list: ArtistList
    let onSelect: (String) -> Void

    var body: some View {
        if let error = list.initialLoadError {
            ListPlaceholder {
                LoadFailureView(line: error.line) {
                    Task { await list.loadInitial() }
                }
            }
        }
        else if list.totalCount == 0 {
            ListPlaceholder {
                Text("No artists")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .padding(32)
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
        HStack(spacing: 12) {
            ImageView(imageRef: summary.image, pointSize: 48)
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 2) {
                Text(summary.name)
                    .font(.body)
                    .lineLimit(1)
                Text("\(summary.albumCount) albums")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
        }
        .padding(.vertical, 4)
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
