import BaeKit
import SwiftUI

/// The composers browse list: one row per loaded slot, paged from the store.
struct ComposerListView: View {
    let list: ComposerList
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
                Text("No composers")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .padding(32)
            }
        }
        else {
            List {
                ForEach(0..<list.totalCount, id: \.self) { position in
                    ComposerRowSlot(
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

private struct ComposerRowSlot: View {
    let list: ComposerList
    let position: Int
    let onSelect: (String) -> Void

    @Environment(LibraryStore.self)
    private var libraryStore

    var body: some View {
        Group {
            if let id = list.idAt(position),
                let summary = libraryStore.composerSummaries[id]
            {
                Button {
                    onSelect(id)
                } label: {
                    ComposerSummaryRow(summary: summary)
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

struct ComposerSummaryRow: View {
    let summary: BridgeComposerSummary

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: summary.image, pointSize: 48)
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 2) {
                Text(summary.name)
                    .font(.body)
                    .lineLimit(1)
                Text("\(summary.workCount) works")
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
        ComposerSummaryRow(summary: PreviewData.composerSummary)
    }
    .previewStores()
}
#endif
