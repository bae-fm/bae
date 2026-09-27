import BaeKit
import SwiftUI

struct SearchView: View {
    /// The card's width, which the overlay reads to align it under the field.
    static let width: CGFloat = 440

    let results: SearchResults?
    let onSelectAlbum: (String) -> Void
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
                    ContentUnavailableView.search(text: results.query)
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
        .background(
            RoundedRectangle(cornerRadius: ThemeRadius.card)
                .fill(Theme.surfaceElevated)
        )
        .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.card))
        .overlay(
            RoundedRectangle(cornerRadius: ThemeRadius.card)
                .strokeBorder(Theme.hairline, lineWidth: 1)
        )
        .shadow(color: Theme.shadow, radius: 18, y: 8)
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
                SearchResultRow(
                    leading: .picture(album.cover),
                    title: album.title,
                    subtitle: albumSubtitle(album),
                    action: { onSelectAlbum(album.id) }
                )
            }
            resultsSection("Artists", results.artists, id: \.id) { artist in
                SearchResultRow(
                    leading: .picture(artist.image),
                    title: artist.name,
                    subtitle:
                        "\(artist.albumCount) \(String(localized: "Albums"))",
                    action: { onSelectArtist(artist.artistId) }
                )
            }
            // Tracks lead with a waveform, since the albums above show the art.
            resultsSection("Tracks", results.tracks, id: \.id) { track in
                SearchResultRow(
                    leading: .waveform,
                    title: track.title,
                    subtitle: trackSubtitle(track),
                    trailing: track.durationLabel.isEmpty
                        ? nil : track.durationLabel,
                    action: { onSelectAlbum(track.albumId) }
                )
            }
            resultsSection("Composers", results.composers, id: \.id) {
                composer in
                SearchResultRow(
                    leading: .picture(composer.image),
                    title: composer.name,
                    subtitle:
                        "\(composer.workCount) \(String(localized: "Works"))",
                    action: { onSelectComposer(composer.id) }
                )
            }
            resultsSection("Works", results.works, id: \.id) { work in
                SearchResultRow(
                    leading: .picture(work.representativeCover),
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
            .padding(.horizontal, ThemeSpace.group)
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

/// One search hit: a picture or waveform, a title over an optional subtitle,
/// and an optional trailing label.
private struct SearchResultRow: View {
    /// A picture, or the waveform glyph that marks a track.
    enum Leading {
        case picture(BridgeImageRef?)
        case waveform
    }

    let leading: Leading
    let title: String
    let subtitle: String?
    var trailing: String?
    let action: () -> Void

    @State
    private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: ThemeSpace.group) {
                switch leading {
                case .picture(let cover):
                    ImageView(imageRef: cover, pointSize: ThemeSize.rowArtwork)
                        .frame(
                            width: ThemeSize.rowArtwork,
                            height: ThemeSize.rowArtwork
                        )
                        .clipShape(
                            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                        )
                case .waveform:
                    Image(systemName: "waveform")
                        .themeIcon(.medium)
                        .foregroundStyle(.secondary)
                        .frame(width: ThemeIcon.medium.size)
                }

                VStack(alignment: .leading, spacing: ThemeSpace.line) {
                    Text(title)
                        .themeText(.rowTitle)
                        .lineLimit(1)
                    StableOptionalText(
                        text: subtitle,
                        font: ThemeText.detail.font,
                        foreground: .secondary,
                        lineHeight: 14,
                        lineLimit: 1
                    )
                }

                Spacer(minLength: ThemeSpace.related)

                if let trailing {
                    Text(trailing)
                        .themeText(.detail)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, ThemeSpace.group)
            .padding(.vertical, ThemeSpace.related)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .background(
                RoundedRectangle(cornerRadius: ThemeRadius.control)
                    .fill(hovering ? Theme.hover : Color.clear)
            )
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("With results") {
        SearchView(
            results: PreviewData.searchResults,
            onSelectAlbum: { _ in },
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
            onSelectArtist: { _ in },
            onSelectComposer: { _ in },
            onSelectWork: { _ in },
        )
        .frame(width: 700, height: 550)
        .environment(ImageStore.stub())
    }
#endif
