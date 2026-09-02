import BaeKit
import SwiftUI

struct SearchView: View {
    /// The card's width. The overlay that places it under the search field
    /// reads this to align their trailing edges.
    static let width: CGFloat = 440

    let results: SearchResults?
    let onSelectAlbum: (String) -> Void
    let onSelectArtist: (String) -> Void
    let onSelectComposer: (String) -> Void
    let onSelectWork: (String) -> Void

    /// Measured natural height of the result list, driving the card's
    /// fit-to-content height.
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
                    // The card hugs its results, scrolling once they outgrow
                    // the cap. The viewport height is the measured content
                    // height, clamped: `.frame(maxHeight:)` can't do this — a
                    // max-frame fills to its cap regardless of content and
                    // centers the child in the leftover space.
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
            RoundedRectangle(cornerRadius: 12).fill(Theme.surfaceElevated)
        )
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .strokeBorder(Color.white.opacity(0.08), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.7), radius: 30, y: 22)
        // The result list's height is measured (`contentHeight` above), so on
        // the mount frame the card would render as a zero-height sliver of
        // chrome before the measurement lands. Hold it invisible until then;
        // the empty state needs no measurement and shows immediately.
        .opacity(showsEmptyState || contentHeight > 0 ? 1 : 0)
    }

    /// Whether the card is showing the fixed-height no-results state rather
    /// than the measured result list.
    private var showsEmptyState: Bool {
        guard let results else {
            return false
        }
        return results.albums.isEmpty && results.artists.isEmpty
            && results.tracks.isEmpty && results.composers.isEmpty
            && results.works.isEmpty
    }

    private func resultsList(_ results: SearchResults) -> some View {
        VStack(alignment: .leading, spacing: 2) {
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
            // A track's row leads with a waveform, not its album's cover: the
            // albums above already show the art, and the glyph says which
            // kind of hit this is.
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
        .padding(8)
    }

    /// One search-result section: a header plus a row per item, rendered only
    /// when the section is non-empty.
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
        Text(title)
            .font(.system(size: 12, weight: .heavy))
            .tracking(0.4)
            .foregroundStyle(.primary.opacity(0.9))
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.top, 14)
            .padding(.bottom, 8)
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

/// One search hit: 46×46 cover art where the hit has a picture, a waveform
/// glyph where it is a track, a title over an optional subtitle, an optional
/// trailing label (a track's duration), with a subtle hover fill.
private struct SearchResultRow: View {
    /// What leads the row: a 46pt picture (the placeholder when the hit has
    /// none yet), or the waveform glyph that marks a track.
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
            HStack(spacing: 14) {
                switch leading {
                case .picture(let cover):
                    ImageView(imageRef: cover, pointSize: 46)
                        .frame(width: 46, height: 46)
                        .clipShape(RoundedRectangle(cornerRadius: 7))
                case .waveform:
                    Image(systemName: "waveform")
                        .font(.system(size: 18, weight: .regular))
                        .foregroundStyle(.secondary)
                        .frame(width: 20)
                }

                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .font(.system(size: 15, weight: .semibold))
                        .lineLimit(1)
                    StableOptionalText(
                        text: subtitle,
                        font: .system(size: 13, weight: .medium),
                        foreground: .secondary,
                        lineHeight: 14,
                        lineLimit: 1
                    )
                }

                Spacer(minLength: 8)

                if let trailing {
                    Text(trailing)
                        .font(
                            .system(size: 13.5, weight: .medium)
                                .monospacedDigit()
                        )
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 9)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(Color.white.opacity(hovering ? 0.05 : 0))
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
