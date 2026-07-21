import BaeKit
import SwiftUI

struct SearchView: View {
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
                if results.albums.isEmpty, results.artists.isEmpty,
                    results.tracks.isEmpty, results.composers.isEmpty,
                    results.works.isEmpty
                {
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
        .frame(width: 572)
        .background(
            RoundedRectangle(cornerRadius: 14)
                .fill(
                    LinearGradient(
                        colors: [Theme.surfaceElevated, Theme.surface],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                )
        )
        .clipShape(RoundedRectangle(cornerRadius: 14))
        .overlay(
            RoundedRectangle(cornerRadius: 14)
                .strokeBorder(Color.white.opacity(0.08), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.5), radius: 24, y: 12)
    }

    private func resultsList(_ results: SearchResults) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            if !results.albums.isEmpty {
                sectionHeader("Albums")
                ForEach(results.albums, id: \.id) { album in
                    SearchResultRow(
                        cover: album.cover,
                        title: album.title,
                        subtitle: albumSubtitle(album),
                        action: { onSelectAlbum(album.id) }
                    )
                }
            }

            if !results.artists.isEmpty {
                sectionHeader("Artists")
                ForEach(results.artists, id: \.id) { artist in
                    SearchResultRow(
                        cover: artist.image,
                        title: artist.name,
                        subtitle:
                            "\(artist.albumCount) \(String(localized: "Albums"))",
                        action: { onSelectArtist(artist.artistId) }
                    )
                }
            }

            if !results.tracks.isEmpty {
                sectionHeader("Tracks")
                ForEach(results.tracks, id: \.id) { track in
                    SearchResultRow(
                        cover: track.cover,
                        title: track.title,
                        subtitle: trackSubtitle(track),
                        trailing: track.durationLabel.isEmpty
                            ? nil : track.durationLabel,
                        action: { onSelectAlbum(track.albumId) }
                    )
                }
            }

            if !results.composers.isEmpty {
                sectionHeader("Composers")
                ForEach(results.composers, id: \.id) { composer in
                    SearchResultRow(
                        cover: composer.image,
                        title: composer.name,
                        subtitle:
                            "\(composer.workCount) \(String(localized: "Works"))",
                        action: { onSelectComposer(composer.id) }
                    )
                }
            }

            if !results.works.isEmpty {
                sectionHeader("Works")
                ForEach(results.works, id: \.id) { work in
                    SearchResultRow(
                        cover: work.representativeCover,
                        title: work.title,
                        subtitle: work.composerNames,
                        action: { onSelectWork(work.id) }
                    )
                }
            }
        }
        .padding(8)
    }

    private func sectionHeader(_ title: LocalizedStringKey) -> some View {
        Text(title)
            .font(.system(size: 12, weight: .heavy))
            .tracking(0.5)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 8)
            .padding(.top, 18)
            .padding(.bottom, 4)
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

/// One search hit: 46×46 cover art (placeholder when absent), a title over an
/// optional subtitle, an optional trailing label (a track's duration), with a
/// subtle hover fill.
private struct SearchResultRow: View {
    let cover: BridgeImageRef?
    let title: String
    let subtitle: String?
    var trailing: String?
    let action: () -> Void

    @State
    private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                ImageView(imageRef: cover, pointSize: 46)
                    .frame(width: 46, height: 46)
                    .clipShape(RoundedRectangle(cornerRadius: 8))

                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .font(.system(size: 16, weight: .semibold))
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
                        .font(.system(size: 14).monospacedDigit())
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(Color.white.opacity(hovering ? 0.06 : 0))
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
        .environment(MediaPaths.stub)
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
        .environment(MediaPaths.stub)
    }
#endif
