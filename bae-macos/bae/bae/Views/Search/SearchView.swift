import BaeKit
import SwiftUI

struct SearchView: View {
    /// The card's width, which the overlay reads to align it under the field.
    static let width: CGFloat = 440

    let results: SearchResults?
    let onSelectAlbum: (String) -> Void
    let onSelectTrack: (TrackSearchResult) -> Void
    let onSelectArtist: (String) -> Void
    let onSelectComposer: (String) -> Void
    let onSelectWork: (String) -> Void

    /// The result list's measured height, which the card fits to.
    @State
    private var contentHeight: CGFloat = 0

    var body: some View {
        Group {
            if let results {
                if showsEmptyState {
                    ContentUnavailableView { Text("No matches") }
                        .frame(height: 240)
                }
                else {
                    // A clamped measured height, since `.frame(maxHeight:)`
                    // would always fill to the cap.
                    ScrollView {
                        resultsList(results)
                            .onGeometryChange(for: CGFloat.self) { geo in
                                geo.size.height
                            } action: {
                                contentHeight = $0
                            }
                    }
                    .frame(height: min(contentHeight, 455))
                }
            }
        }
        .frame(width: Self.width)
        .card(elevated: true)
        .overlay {
            RoundedRectangle(cornerRadius: ThemeRadius.card)
                .strokeBorder(Theme.hairline, lineWidth: 1)
        }
        // Hidden until the list is measured, so it never shows as a sliver.
        .opacity(showsEmptyState || contentHeight > 0 ? 1 : 0)
    }

    /// Whether the card shows the no-results state.
    private var showsEmptyState: Bool {
        guard let results else {
            return false
        }
        return results.albums.isEmpty && results.artists.isEmpty
            && results.tracks.isEmpty && results.composers.isEmpty
            && results.works.isEmpty
    }

    private func resultsList(_ results: SearchResults) -> some View {
        VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
            resultsSection("Albums", results.albums, id: \.id) { album in
                MacLibraryRowButton(
                    picture: album.cover,
                    title: album.title,
                    subtitle: albumSubtitle(album),
                    action: { onSelectAlbum(album.id) }
                )
            }
            resultsSection("Artists", results.artists, id: \.id) { artist in
                MacLibraryRowButton(
                    picture: artist.image,
                    title: artist.name,
                    subtitle: artist.countLine.localizedLine,
                    action: { onSelectArtist(artist.artistId) }
                )
            }
            resultsSection("Tracks", results.tracks, id: \.id) { track in
                MacLibraryRowButton(
                    picture: track.cover,
                    title: track.title,
                    subtitle: trackSubtitle(track),
                    trailing: track.durationLabel,
                    action: { onSelectTrack(track) }
                )
            }
            resultsSection("Composers", results.composers, id: \.id) {
                composer in
                MacLibraryRowButton(
                    picture: composer.image,
                    title: composer.name,
                    subtitle: composer.countLine.localizedLine,
                    action: { onSelectComposer(composer.id) }
                )
            }
            resultsSection("Works", results.works, id: \.id) { work in
                MacLibraryRowButton(
                    picture: work.representativeCover,
                    title: work.title,
                    subtitle: work.composerNames,
                    action: { onSelectWork(work.id) }
                )
            }
        }
        .padding(ThemeSpace.related)
    }

    /// A header and a row per item, omitted when there are no items.
    @ViewBuilder
    private func resultsSection<Item, ID: Hashable, Row: View>(
        _ title: LocalizedStringKey,
        _ items: [Item],
        id: KeyPath<Item, ID>,
        @ViewBuilder row: @escaping (Item) -> Row
    ) -> some View {
        if !items.isEmpty {
            sectionHeader(title)
            ForEach(items, id: id) { row($0) }
        }
    }

    private func sectionHeader(_ title: LocalizedStringKey) -> some View {
        Eyebrow(title)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, ThemeSpace.related)
            .padding(.top, ThemeSpace.group)
            .padding(.bottom, ThemeSpace.related)
    }

    private func albumSubtitle(_ album: AlbumSearchResult) -> String {
        if let year = album.year {
            "\(album.artistName) (\(year))"
        }
        else {
            album.artistName
        }
    }

    private func trackSubtitle(_ track: TrackSearchResult) -> String {
        String(
            format: String(localized: "%@ - %@"),
            track.artistName,
            track.albumTitle
        )
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("With results") {
        SearchView(
            results: PreviewData.searchResults,
            onSelectAlbum: { _ in },
            onSelectTrack: { _ in },
            onSelectArtist: { _ in },
            onSelectComposer: { _ in },
            onSelectWork: { _ in },
        )
        .frame(width: 700, height: 600)
        .environment(ImageStore.stub())
    }

    #Preview("No results") {
        SearchView(
            results: SearchResults(
                bridge: BridgeSearchResults(
                    albums: [],
                    artists: [],
                    tracks: [],
                    composers: [],
                    works: []
                ),
                query: "placeholder"
            ),
            onSelectAlbum: { _ in },
            onSelectTrack: { _ in },
            onSelectArtist: { _ in },
            onSelectComposer: { _ in },
            onSelectWork: { _ in },
        )
        .frame(width: 700, height: 550)
        .environment(ImageStore.stub())
    }
#endif
