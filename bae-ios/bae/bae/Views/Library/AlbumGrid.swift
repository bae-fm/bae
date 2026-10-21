import BaeKit
import SwiftUI

/// The narrowest a grid cell gets, and the size its cover is decoded at.
private let cellWidth: CGFloat = 150

/// The paged album grid: one cell per slot, each holding its own page.
struct AlbumGrid: View {
    let list: AlbumList
    var groupByArtist = false
    let onSelect: (String) -> Void

    @Environment(SyncActivityStore.self)
    private var syncActivity

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
                    if groupByArtist {
                        ForEach(list.sections, id: \.id) { section in
                            Section {
                                cells(
                                    Int(
                                        section.window.offset
                                    )..<Int(section.window.offset + section.window.limit)
                                )
                            } header: {
                                Text(verbatim: section.title)
                                    .themeText(.rowTitle)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .padding(.top, ThemeSpace.group)
                                    .accessibilityAddTraits(.isHeader)
                            }
                        }
                    }
                    else {
                        cells(0..<list.totalCount)
                    }
                }
                .padding(ThemeSpace.group)
            }
        }
    }

    private func cells(_ positions: Range<Int>) -> some View {
        ForEach(positions, id: \.self) { position in
            AlbumCell(
                list: list,
                position: position,
                arrivingAlbums: syncActivity.arrivingAlbums,
                onSelect: onSelect
            )
        }
    }
}

/// One grid slot: holds its page while it is shown, again whenever `loadEpoch`
/// changes, and shows the album once it resolves.
private struct AlbumCell: View {
    let list: AlbumList
    let position: Int
    /// Each album still arriving from another device, with that device.
    let arrivingAlbums: [String: String]
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
                    arrivingFrom: arrivingAlbums[albumId],
                    onTap: { onSelect(albumId) }
                )
            }
            else {
                Rectangle().fill(Theme.placeholder)
                    .aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
            }
        }
        .task(id: RowLoadID(epoch: list.loadEpoch, index: position)) {
            await list.holdPage(containing: position)
        }
    }
}

private struct AlbumCard: View {
    let summary: AlbumSummary
    /// The device the album's files are still arriving from, if any are.
    let arrivingFrom: String?
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: ThemeSpace.compact) {
                ImageView(imageRef: summary.cover, pointSize: cellWidth)
                    .aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
                    .overlay(alignment: .bottomTrailing) {
                        AlbumArrivalBadge(device: arrivingFrom)
                            .padding(ThemeSpace.compact)
                    }
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
        .accessibilityValue(arrivingFrom.map(BridgeArrivals.fromLine) ?? "")
    }
}

#if DEBUG
#Preview {
    PreviewScenes.libraryGrid()
}
#endif
